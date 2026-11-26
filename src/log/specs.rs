//! What Verus assumes of the types and functions the log uses from outside the proof.
//!
//! Each declaration here is trusted: Verus takes it as written, without checking the code
//! it describes.

use std::cell::UnsafeCell;

use crossbeam_epoch as epoch;
use smallvec::SmallVec;
use vstd::prelude::*;

use super::Evicted;

verus! {

/// The error a read returns.
#[verifier::external_type_specification]
pub struct ExEvicted(Evicted);

/// Verus's view of `smallvec::Array`.
///
/// # Safety
///
/// As `smallvec::Array`; Verus only names it.
#[verifier::external_trait_specification]
pub unsafe trait ExArray {
	type ExternalTraitSpecificationFor: smallvec::Array;

	type Item;
}

/// A slot's storage, which only trusted code touches.
#[verifier::external_type_specification]
#[verifier::external_body]
#[verifier::accept_recursive_types(T)]
pub struct ExUnsafeCell<T: ?Sized>(UnsafeCell<T>);

/// An epoch pin, which the proof holds but does not model.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExGuard(epoch::Guard);

/// The writer's list of live segments, seen as the sequence [`items`] returns.
#[verifier::external_type_specification]
#[verifier::external_body]
#[verifier::reject_recursive_types(A)]
pub struct ExSmallVec<A: smallvec::Array>(SmallVec<A>);

/// The items a `SmallVec` holds.
pub uninterp spec fn items<A: smallvec::Array>(list: &SmallVec<A>) -> Seq<A::Item>;

pub assume_specification<A: smallvec::Array>[ SmallVec::<A>::push ](list: &mut SmallVec<A>, item: A::Item)
	ensures
		items(final(list)) == items(old(list)).push(item),
;

pub assume_specification<A: smallvec::Array>[ <SmallVec<A> as core::ops::Deref>::deref ](list: &SmallVec<A>) -> (slice: &[A::Item])
	ensures
		slice@ == items(list),
;

/// TRUSTED: move the first `count` items out of `list`.
#[verifier::external_body]
pub fn drain_front<A: smallvec::Array>(list: &mut SmallVec<A>, count: usize) -> (front: SmallVec<[A::Item; 1]>)
	requires
		count <= items(old(list)).len(),
	ensures
		items(&front) == items(old(list)).take(count as int),
		items(final(list)) == items(old(list)).skip(count as int),
{
	list.drain(..count).collect()
}

pub assume_specification<A: smallvec::Array>[ SmallVec::<A>::new ]() -> (list: SmallVec<A>)
	ensures
		items(&list) == Seq::<A::Item>::empty(),
;

pub assume_specification<A: smallvec::Array>[ SmallVec::<A>::is_empty ](list: &SmallVec<A>) -> (empty: bool)
	ensures
		empty == (items(list).len() == 0),
;

} // verus!
