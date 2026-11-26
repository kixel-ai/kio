// A reactor with nothing to fire.
struct R;

#[kio_macros::reactor]
impl R {
	#[invariant]
	fn holds(&self) -> bool {
		true
	}
}

fn main() {}
