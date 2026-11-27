//! The adversarial-scheduling spike's fixture: a poll-based `SendCredit` session run on
//! `kio::clock::sim`, under whichever schedule and probe `KIO_SIM_SCHEDULE` and
//! `KIO_SIM_PROBE` name.
//!
//! `cargo run --release -p kio-adversarial-lab --example report` prints the spike's numbers.

mod send_credit;

pub use send_credit::{Bug, Fixture, run};

/// Set `KIO_SIM_SCHEDULE` and `KIO_SIM_PROBE` for the runs that follow.
///
/// # Safety
///
/// No other thread may read or write the environment meanwhile, as
/// [`std::env::set_var`] requires.
pub unsafe fn configure(schedule: &str, probe: bool) {
	// SAFETY: the caller guarantees no other thread touches the environment.
	unsafe {
		std::env::set_var("KIO_SIM_SCHEDULE", schedule);
		std::env::set_var("KIO_SIM_PROBE", if probe { "1" } else { "0" });
	}
}
