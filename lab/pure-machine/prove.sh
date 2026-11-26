#!/usr/bin/env bash
# Translate the pure machines to Lean with Charon and Aeneas, fail if the translation in
# proofs/PureMachine differs from it, then check every proof in proofs/.
#
# Usage: nix develop .#prove --command lab/pure-machine/prove.sh
# With --write, replace the committed translation instead of failing on a difference.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"
out=$(git rev-parse --show-toplevel)/.tmp/prove
mkdir -p "$out"
# Charon builds against its own standard library, so keep its build apart.
export CARGO_TARGET_DIR=$out/target

charon cargo --preset=aeneas \
	--start-from 'crate::send_credit::Session::step' \
	--start-from 'crate::send_credit::Session::new' \
	--start-from 'crate::send_credit::Session::within_credit' \
	--start-from 'crate::send_credit::Session::well_formed' \
	--start-from 'crate::warm_settle::Relay::next_pulls' \
	--start-from 'crate::warm_settle::best' \
	--dest-file "$out/pure.llbc" -- -p kio-pure-machine --lib
rm -rf "$out/lean"
aeneas -backend lean -split-files -abort-on-error -no-progress-bar \
	-dest "$out/lean" -subdir PureMachine -namespace pure_machine "$out/pure.llbc"

for file in Types Funs; do
	if [ "${1:-}" = --write ]; then
		cp "$out/lean/PureMachine/$file.lean" "proofs/PureMachine/$file.lean"
	else
		diff -u "proofs/PureMachine/$file.lean" "$out/lean/PureMachine/$file.lean"
	fi
done

cd proofs
lake exe cache get
lake build
