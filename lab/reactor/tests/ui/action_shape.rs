// An action that cannot fire: it borrows the reactor immutably.
use std::task::Poll;

struct R;

#[kio_macros::reactor]
impl R {
	#[action]
	fn step(&self, waiter: &kio::Waiter) -> Poll<()> {
		let _ = waiter;
		Poll::Pending
	}
}

fn main() {}
