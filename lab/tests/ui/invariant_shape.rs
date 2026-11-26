// An invariant that says why: it must answer only whether it holds.
use std::task::Poll;

struct R;

#[kio_macros::reactor]
impl R {
	#[action]
	fn step(&mut self, waiter: &kio::Waiter) -> Poll<()> {
		let _ = waiter;
		Poll::Pending
	}

	#[invariant]
	fn holds(&self) -> Result<(), String> {
		Ok(())
	}
}

fn main() {}
