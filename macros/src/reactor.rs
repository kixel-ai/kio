//! The `#[reactor]` expansion: a priority-ordered poll loop over named guarded actions, and
//! under the consuming crate's `sim` feature, each action and invariant on its own.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::{FnArg, Ident, ImplItem, ImplItemFn, ItemImpl, ReturnType, Type, Visibility};

/// The impl with its markers removed, then what it generates, or the error that stops it, so
/// a rejected reactor still defines its methods and reports nothing else.
pub(crate) fn expand(args: TokenStream, mut item: ItemImpl) -> TokenStream {
	let mut marked = Vec::new();
	let mut error: Option<syn::Error> = None;
	for impl_item in &mut item.items {
		let ImplItem::Fn(method) = impl_item else { continue };
		match take_marker(method) {
			Ok(Some(marker)) => marked.push((marker, method.clone())),
			Ok(None) => {}
			Err(err) => match &mut error {
				Some(error) => error.combine(err),
				None => error = Some(err),
			},
		}
	}
	let generated = match error {
		Some(error) => Err(error),
		None => generate(args, &item, &marked),
	};
	let mut out = item.to_token_stream();
	out.extend(generated.unwrap_or_else(syn::Error::into_compile_error));
	out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Marker {
	Action,
	Invariant,
}

fn generate(args: TokenStream, item: &ItemImpl, marked: &[(Marker, ImplItemFn)]) -> syn::Result<TokenStream> {
	if !args.is_empty() {
		return Err(syn::Error::new_spanned(args, "#[reactor] takes no arguments"));
	}
	if let Some((_, trait_, _)) = &item.trait_ {
		return Err(syn::Error::new_spanned(trait_, "#[reactor] goes on an inherent impl"));
	}
	let Type::Path(self_ty) = &*item.self_ty else {
		return Err(syn::Error::new_spanned(&item.self_ty, "#[reactor] needs a named type"));
	};
	let base = &self_ty.path.segments.last().expect("a path has a segment").ident;
	let action_ty = format_ident!("{base}Action");

	let mut actions: Vec<(Ident, Visibility)> = Vec::new();
	let mut invariants: Vec<Ident> = Vec::new();
	for (marker, method) in marked {
		match marker {
			Marker::Action => {
				check_action(method)?;
				actions.push((method.sig.ident.clone(), method.vis.clone()));
			}
			Marker::Invariant => {
				check_invariant(method)?;
				invariants.push(method.sig.ident.clone());
			}
		}
	}
	let Some((_, vis)) = actions.first() else {
		return Err(syn::Error::new(
			Span::call_site(),
			"a #[reactor] needs at least one `#[action]`",
		));
	};
	let vis = vis.clone();

	let variants: Vec<Ident> = actions.iter().map(|(name, _)| camel(name)).collect();
	let methods: Vec<&Ident> = actions.iter().map(|(name, _)| name).collect();
	let labels: Vec<String> = methods.iter().map(ToString::to_string).collect();
	let count = actions.len();
	let invariant_labels: Vec<String> = invariants.iter().map(ToString::to_string).collect();
	let (impl_generics, _, where_clause) = item.generics.split_for_impl();
	let self_ty = &item.self_ty;
	let doc = format!("An action of [`{base}`]'s reactor, in priority order.");

	Ok(quote! {
		#[doc = #doc]
		#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
		#vis enum #action_ty {
			#(
				#[doc = concat!("`", #labels, "`.")]
				#variants,
			)*
		}

		impl #action_ty {
			/// Every action, highest priority first.
			#vis const ALL: [Self; #count] = [#(Self::#variants),*];

			/// Every action's method name, in [`Self::ALL`]'s order.
			#vis const NAMES: [&'static str; #count] = [#(#labels),*];

			/// The action's method name.
			#vis fn name(self) -> &'static str {
				match self {
					#(Self::#variants => #labels,)*
				}
			}
		}

		impl #impl_generics #self_ty #where_clause {
			/// Fire the highest-priority enabled action, or park every one that is disabled.
			///
			/// Each action that returns `Poll::Pending` registers `waiter` for what would
			/// enable it, so a pending result parks the whole reactor. Under `sim`, every
			/// invariant is checked after the action fires.
			#[inline]
			#vis fn poll_next(&mut self, waiter: &::kio::Waiter) -> ::std::task::Poll<#action_ty> {
				#(
					if self.#methods(waiter).is_ready() {
						#[cfg(feature = "sim")]
						if let ::std::option::Option::Some(name) = self.violated() {
							panic!("invariant `{}` broken by action `{}`", name, #labels);
						}
						return ::std::task::Poll::Ready(#action_ty::#variants);
					}
				)*
				::std::task::Poll::Pending
			}

			/// Fire `action` alone if it is enabled, ignoring priority.
			#[cfg(feature = "sim")]
			#vis fn poll_action(&mut self, action: #action_ty, waiter: &::kio::Waiter) -> ::std::task::Poll<()> {
				match action {
					#(#action_ty::#variants => self.#methods(waiter),)*
				}
			}

			/// The first invariant that does not hold, by method name.
			#[cfg(feature = "sim")]
			#vis fn violated(&self) -> ::std::option::Option<&'static str> {
				#(
					if !self.#invariants() {
						return ::std::option::Option::Some(#invariant_labels);
					}
				)*
				::std::option::Option::None
			}
		}
	})
}

/// Remove a method's bare `#[action]` or `#[invariant]` marker, reporting which it was.
fn take_marker(method: &mut ImplItemFn) -> syn::Result<Option<Marker>> {
	let mut found = None;
	let mut error = None;
	method.attrs.retain(|attr| {
		let marker = if attr.path().is_ident("action") {
			Marker::Action
		} else if attr.path().is_ident("invariant") {
			Marker::Invariant
		} else {
			return true;
		};
		let err = if let Err(err) = attr.meta.require_path_only() {
			Some(err)
		} else if found.is_some() {
			Some(syn::Error::new_spanned(
				attr,
				"a method is one `#[action]` or one `#[invariant]`",
			))
		} else {
			None
		};
		error = error.take().or(err);
		found = Some(marker);
		false
	});
	match error {
		Some(err) => Err(err),
		None => Ok(found),
	}
}

/// `fn name(&mut self, waiter: &Waiter) -> Poll<()>`.
fn check_action(method: &ImplItemFn) -> syn::Result<()> {
	let sig = &method.sig;
	let shape = "an `#[action]` is `fn name(&mut self, waiter: &kio::Waiter) -> Poll<()>`";
	if sig.asyncness.is_some() || sig.constness.is_some() || sig.unsafety.is_some() || !sig.generics.params.is_empty() {
		return Err(syn::Error::new_spanned(&sig.ident, shape));
	}
	let mut inputs = sig.inputs.iter();
	match inputs.next() {
		Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() && receiver.mutability.is_some() => {}
		Some(other) => return Err(syn::Error::new_spanned(other, shape)),
		None => return Err(syn::Error::new_spanned(&sig.inputs, shape)),
	}
	match (inputs.next(), inputs.next()) {
		(Some(FnArg::Typed(waiter)), None) if is_ref_to(&waiter.ty, "Waiter") => {}
		_ => return Err(syn::Error::new(sig.paren_token.span.join(), shape)),
	}
	match &sig.output {
		ReturnType::Type(_, ty) if is_poll_unit(ty) => Ok(()),
		ReturnType::Type(_, ty) => Err(syn::Error::new_spanned(ty, shape)),
		ReturnType::Default => Err(syn::Error::new_spanned(&sig.ident, shape)),
	}
}

/// `fn name(&self) -> bool`.
fn check_invariant(method: &ImplItemFn) -> syn::Result<()> {
	let sig = &method.sig;
	let shape = "an `#[invariant]` is `fn name(&self) -> bool`";
	let receiver = match sig.inputs.first() {
		Some(FnArg::Receiver(receiver)) => receiver.reference.is_some() && receiver.mutability.is_none(),
		_ => false,
	};
	let output = matches!(&sig.output, ReturnType::Type(_, ty) if ends_in(ty, "bool"));
	if !receiver || sig.inputs.len() != 1 || !output || sig.asyncness.is_some() || !sig.generics.params.is_empty() {
		return Err(syn::Error::new_spanned(&sig.ident, shape));
	}
	Ok(())
}

/// `Poll<()>`, by any path ending in `Poll`.
fn is_poll_unit(ty: &Type) -> bool {
	let Type::Path(path) = ty else { return false };
	let Some(last) = path.path.segments.last() else {
		return false;
	};
	let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
		return false;
	};
	last.ident == "Poll"
		&& args.args.len() == 1
		&& matches!(args.args.first(), Some(syn::GenericArgument::Type(Type::Tuple(unit))) if unit.elems.is_empty())
}

fn is_ref_to(ty: &Type, name: &str) -> bool {
	matches!(ty, Type::Reference(reference) if reference.mutability.is_none() && ends_in(&reference.elem, name))
}

fn ends_in(ty: &Type, name: &str) -> bool {
	matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == name))
}

/// `hand_out` becomes `HandOut`.
fn camel(name: &Ident) -> Ident {
	let text = name.to_string();
	let camel: String = text
		.trim_start_matches("r#")
		.split('_')
		.filter(|part| !part.is_empty())
		.map(|part| {
			let mut chars = part.chars();
			let first = chars.next().expect("split filtered empty parts");
			first.to_uppercase().chain(chars).collect::<String>()
		})
		.collect();
	Ident::new(&camel, name.span())
}
