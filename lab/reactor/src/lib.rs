//! Experiments on `kio` state-machine designs, never published.
//!
//! [`send_credit`] is `moq-net`'s `SendCredit.tla` written with the experimental
//! `#[reactor]` macro, and [`explore`] fires its actions in any order to find what the
//! priority order hides.

#[cfg(feature = "sim")]
pub mod explore;
pub mod send_credit;
