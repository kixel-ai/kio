//! The log's atomics, with the ghost permissions Verus reasons about.
//!
//! Each type wraps a std atomic and, under Verus, pairs it with a permission whose
//! `value` is the atomic's current value: an operation needs the permission, so the
//! proof always knows what a load returns. Plain builds erase the permissions; the
//! wrappers compile to the std operation with the ordering the caller names.
//!
//! These bodies are trusted. Verus models every atomic access as sequentially
//! consistent, so the proof assumes the orderings the log names carry what it
//! publishes: a `Release` store makes the writes before it visible to the `Acquire`
//! load that reads it. The stores require `Release` so a weakened publish fails the
//! proof; whether each load's ordering suffices is Miri's and GenMC's to check.

use std::sync::atomic::{self, Ordering};

use vstd::prelude::*;

verus! {

/// An `AtomicUsize` whose value the proof tracks through a [`UsizePerm`].
#[verifier::external_body]
pub struct Usize(atomic::AtomicUsize);

/// The permission to access a [`Usize`], and its value.
#[verifier::external_body]
pub tracked struct UsizePerm {
	_private: (),
}

impl UsizePerm {
	/// The atomic this permission is for.
	pub uninterp spec fn id(&self) -> int;

	/// The atomic's value.
	pub uninterp spec fn value(&self) -> usize;
}

impl Usize {
	/// Which atomic this is, matched against a permission's.
	pub uninterp spec fn id(&self) -> int;

	#[inline]
	#[verifier::external_body]
	pub fn new(value: usize) -> (result: (Self, Tracked<UsizePerm>))
		ensures
			result.1@.id() == result.0.id(),
			result.1@.value() == value,
	{
		(Self(atomic::AtomicUsize::new(value)), Tracked::assume_new())
	}

	#[inline]
	#[verifier::external_body]
	#[verifier::atomic]
	pub fn load(&self, Tracked(perm): Tracked<&UsizePerm>, order: Ordering) -> (value: usize)
		requires
			perm.id() == self.id(),
		ensures
			value == perm.value(),
		opens_invariants none
		no_unwind
	{
		self.0.load(order)
	}

	#[inline]
	#[verifier::external_body]
	#[verifier::atomic]
	pub fn store(&self, Tracked(perm): Tracked<&mut UsizePerm>, value: usize, order: Ordering)
		requires
			old(perm).id() == self.id(),
			order == Ordering::Release,
		ensures
			final(perm).id() == self.id(),
			final(perm).value() == value,
		opens_invariants none
		no_unwind
	{
		self.0.store(value, order)
	}
}

/// An `AtomicU64` whose value the proof tracks through a [`U64Perm`].
#[verifier::external_body]
pub struct U64(atomic::AtomicU64);

/// The permission to access a [`U64`], and its value.
#[verifier::external_body]
pub tracked struct U64Perm {
	_private: (),
}

impl U64Perm {
	/// The atomic this permission is for.
	pub uninterp spec fn id(&self) -> int;

	/// The atomic's value.
	pub uninterp spec fn value(&self) -> u64;
}

impl U64 {
	/// Which atomic this is, matched against a permission's.
	pub uninterp spec fn id(&self) -> int;

	#[inline]
	#[verifier::external_body]
	pub fn new(value: u64) -> (result: (Self, Tracked<U64Perm>))
		ensures
			result.1@.id() == result.0.id(),
			result.1@.value() == value,
	{
		(Self(atomic::AtomicU64::new(value)), Tracked::assume_new())
	}

	#[inline]
	#[verifier::external_body]
	#[verifier::atomic]
	pub fn load(&self, Tracked(perm): Tracked<&U64Perm>, order: Ordering) -> (value: u64)
		requires
			perm.id() == self.id(),
		ensures
			value == perm.value(),
		opens_invariants none
		no_unwind
	{
		self.0.load(order)
	}

	#[inline]
	#[verifier::external_body]
	#[verifier::atomic]
	pub fn store(&self, Tracked(perm): Tracked<&mut U64Perm>, value: u64, order: Ordering)
		requires
			old(perm).id() == self.id(),
			order == Ordering::Release,
		ensures
			final(perm).id() == self.id(),
			final(perm).value() == value,
		opens_invariants none
		no_unwind
	{
		self.0.store(value, order)
	}
}

/// An `AtomicPtr<T>` whose value the proof tracks through a [`PtrPerm`].
#[verifier::external_body]
#[verifier::accept_recursive_types(T)]
pub struct Ptr<T>(atomic::AtomicPtr<T>);

/// The permission to access a [`Ptr`], and its value.
#[verifier::external_body]
#[verifier::accept_recursive_types(T)]
pub tracked struct PtrPerm<T> {
	_private: core::marker::PhantomData<T>,
}

impl<T> PtrPerm<T> {
	/// The atomic this permission is for.
	pub uninterp spec fn id(&self) -> int;

	/// The atomic's value.
	pub uninterp spec fn value(&self) -> *mut T;
}

impl<T> Ptr<T> {
	/// Which atomic this is, matched against a permission's.
	pub uninterp spec fn id(&self) -> int;

	#[inline]
	#[verifier::external_body]
	pub fn new(value: *mut T) -> (result: (Self, Tracked<PtrPerm<T>>))
		ensures
			result.1@.id() == result.0.id(),
			result.1@.value() == value,
	{
		(Self(atomic::AtomicPtr::new(value)), Tracked::assume_new())
	}

	#[inline]
	#[verifier::external_body]
	#[verifier::atomic]
	pub fn load(&self, Tracked(perm): Tracked<&PtrPerm<T>>, order: Ordering) -> (value: *mut T)
		requires
			perm.id() == self.id(),
		ensures
			value == perm.value(),
		opens_invariants none
		no_unwind
	{
		self.0.load(order)
	}

	#[inline]
	#[verifier::external_body]
	#[verifier::atomic]
	pub fn store(&self, Tracked(perm): Tracked<&mut PtrPerm<T>>, value: *mut T, order: Ordering)
		requires
			old(perm).id() == self.id(),
			order == Ordering::Release,
		ensures
			final(perm).id() == self.id(),
			final(perm).value() == value,
		opens_invariants none
		no_unwind
	{
		self.0.store(value, order)
	}
}

} // verus!

// Outside the proof: code the proof doesn't cover reaches the atomic directly, and is
// trusted to keep the log's invariant as the verified code does.
impl Usize {
	#[inline]
	pub fn unverified(&self) -> &atomic::AtomicUsize {
		&self.0
	}
}
