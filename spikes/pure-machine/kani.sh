#!/usr/bin/env bash
# Run the Kani harnesses, installing Kani into target/kani on first use: nixpkgs has no
# Kani, and `cargo kani setup` installs its pinned nightly with rustup, here a rustup from
# nixpkgs whose homes stay in target/.
#
# Usage: spikes/pure-machine/kani.sh [cargo kani arguments, such as --harness NAME]
set -euo pipefail

root=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
cd "$root"
export RUSTUP_HOME=$root/target/kani/rustup CARGO_HOME=$root/target/kani/cargo KANI_HOME=$root/target/kani/home
export PATH=$CARGO_HOME/bin:$PATH
version=0.68.0

if [ ! -x "$CARGO_HOME/bin/cargo-kani" ] || [ ! -d "$KANI_HOME/kani-$version" ]; then
	nix shell nixpkgs#rustup --command bash -euo pipefail -c "
		rustup toolchain install stable --profile minimal
		rustup default stable
		cargo install --locked kani-verifier --version $version
		cargo kani setup"
fi
exec nix shell nixpkgs#rustup --command cargo kani -p kio-pure-machine "$@"
