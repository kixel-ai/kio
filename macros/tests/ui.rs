//! Each mistake in `ui/` and the diagnostic it reports.

#[test]
fn ui() {
	// Compiler wrappers change diagnostic types and paths. Prevent nested Cargo
	// from restoring mbx's saved environment and reintroducing those wrappers.
	const CHILD: &str = "KIO_UI_TEST_CHILD";
	const REMOVE: &[&str] = &["RUSTC_WORKSPACE_WRAPPER", "RUSTC_WRAPPER", "MBX_SESSION_RESTORE"];
	let test = std::thread::current()
		.name()
		.expect("test runs on its own thread")
		.to_owned();
	let clean = REMOVE.iter().all(|name| std::env::var_os(name).is_none())
		&& std::env::var_os("MBX_DISABLE").as_deref() == Some(std::ffi::OsStr::new("1"));
	if !clean && std::env::var_os(CHILD).as_deref() != Some(std::ffi::OsStr::new(&test)) {
		let mut cmd = std::process::Command::new(std::env::current_exe().expect("test binary"));
		cmd.args([&test, "--exact", "--include-ignored", "--test-threads=1"])
			.env(CHILD, &test)
			.env("MBX_DISABLE", "1");
		for name in REMOVE {
			cmd.env_remove(name);
		}
		let output = cmd.output().expect("failed to start test binary");
		let stdout = String::from_utf8_lossy(&output.stdout);
		print!("{stdout}");
		eprint!("{}", String::from_utf8_lossy(&output.stderr));
		assert!(output.status.success(), "{test} failed in child: {}", output.status);
		assert!(stdout.contains("running 1 test"), "child ran no test named {test}");
		return;
	}
	trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
