// An action ready with a value: a fired action is a transition, not a result.
use std::task::Poll;

struct R;

#[kio_macros::reactor]
impl R {
	#[action]
	fn step(&mut self, waiter: &kio::Waiter) -> Poll<u64> {
		let _ = waiter;
		Poll::Ready(1)
	}
}

fn main() {}
