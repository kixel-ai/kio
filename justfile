set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available development commands.
default:
    @just --list

# Lint the workspace, feature matrix, unused dependencies, rustdoc, workflows, TOML, Nix, and quest tree.
check:
    cargo shear --locked --deny-warnings
    cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
    cargo clippy --locked -p kio --no-default-features -- -D warnings
    cargo clippy --locked -p kio-reactor-lab -- -D warnings
    cargo fmt --all --check
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --workspace --all-features
    actionlint
    taplo format --check
    nixfmt --check flake.nix nix/verus.nix
    quest check

# Run unit, doc, and trybuild UI tests across the workspace, failing on any host-clock timer.
test *args:
    KIO_DENY_HOST_TIMERS=1 cargo test --locked --workspace --all-features {{ args }}

# Run kio's unit tests under Miri (nightly with the miri component).
miri:
    #!/usr/bin/env bash
    set -euo pipefail
    # Fail on a pass that ran no tests, so a renamed module cannot silently empty it.
    run() {
        local out
        out=$(mktemp)
        "$@" 2>&1 | tee "$out"
        grep -Eq 'test result: ok\. [1-9][0-9]* passed' "$out" || { echo "error: no tests ran: $*" >&2; exit 1; }
    }
    # Stacked Borrows with leak checking, everywhere but the log.
    run cargo miri test --locked -p kio --lib -- --skip log::
    # crossbeam-epoch's collector violates Stacked Borrows and never frees at exit.
    MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-ignore-leaks" run cargo miri test --locked -p kio --lib log::

# Run kio's thread-spawning unit tests under Miri across many scheduler seeds (nightly with miri). Random schedules, not exhaustive: a pass is evidence, not proof.
miri-seeds seeds="8":
    #!/usr/bin/env bash
    set -euo pipefail
    # Every library test that spawns a thread. A rename fails the count below, not silently.
    names=(
        channel::tests::racing_publishes_and_closes_wake_every_waiter
        clock::captured::tests::a_paused_runtime_reads_the_same_everywhere
        clock::deadline::tests::entering_nests
        clock::installed::tests::an_entered_queue_keeps_the_installed_clock
        clock::random::tests::the_host_seed_holds_while_draws_move
        clock::sim::tests::the_probe_fails_a_wake_from_another_thread
        clock::sim::tests::the_probe_fails_a_wake_from_another_thread_as_the_root_finishes
        task::tests::a_wake_during_thread_teardown_reaches_the_owner
        task::tests::concurrent_wakes_are_never_lost
        tests::consumer_creation_with_existing_demand_does_not_relock
        weak::test::upgrade_never_wins_a_closing_channel
    )
    run() {
        local out count=$1
        shift
        out=$(mktemp)
        "$@" 2>&1 | tee "$out"
        # Seeds run in parallel, so their result lines can interleave: match loosely.
        grep -Eq "ok\. $count passed" "$out" || { echo "error: expected $count tests per seed: $*" >&2; exit 1; }
    }
    echo "seeds 0..{{ seeds }}"
    # Stacked Borrows with leak checking.
    MIRIFLAGS="-Zmiri-many-seeds=0..{{ seeds }}" run "${#names[@]}" cargo miri test --locked -p kio --lib -- --exact "${names[@]}"
    # The log on Tree Borrows, as `miri` runs it.
    MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-ignore-leaks -Zmiri-many-seeds=0..{{ seeds }}" run 1 cargo miri test --locked -p kio --lib -- --exact log::test::concurrent_readers_see_every_entry

# Run kio's library tests under ThreadSanitizer (nightly with rust-src, Linux). Observes the schedules that happen to run, not all of them: a pass is evidence, not proof. Races are suppressed only by tsan.supp.
tsan:
    #!/usr/bin/env bash
    set -euo pipefail
    host=$(rustc -vV | sed -n 's/^host: //p')
    # An explicit --target keeps the sanitizer off build scripts and proc macros, and -Zbuild-std
    # instruments std, so its locks and atomics are modeled rather than invisible.
    RUSTFLAGS="-Zsanitizer=thread" TSAN_OPTIONS="suppressions={{ justfile_directory() }}/tsan.supp" \
        cargo test --locked -p kio --lib --all-features -Zbuild-std --target "$host" --target-dir target/tsan

# Run what CI runs: lint, then test.
ci: check test

# Validate kio-reactor-lab's explored SendCredit runs with TLC, and model-check the split spec they follow.
lab-tlc seeds="200":
    #!/usr/bin/env bash
    set -euo pipefail
    dir="$(mktemp -d)"
    trap 'rm -rf "$dir"' EXIT
    cargo run --locked -q -p kio-reactor-lab --example traces -- "$dir" {{ seeds }}
    for model in "$dir"/*/; do
        for cfg in Trace Split; do
            spec=$([ "$cfg" = Trace ] && echo SendCreditTrace || echo SendCreditSplit)
            echo "== $(basename "$model") $cfg"
            if ! (cd "$model" && nix shell --inputs-from {{ justfile_directory() }} nixpkgs#tlaplus --command \
                tlc -workers auto -cleanup -metadir "$dir/meta" -config "$cfg.cfg" "$spec.tla") >"$dir/tlc.log" 2>&1; then
                cat "$dir/tlc.log"
                exit 1
            fi
            grep -E "distinct states found|^Finished in" "$dir/tlc.log"
        done
    done

# Prove src/log.rs's Verus proof. Needs Verus: run it in `nix develop .#verus`. The
# `verify` package builds kio's sources with `vstd`, which kio itself never depends on.
verify:
    cargo verus verify --locked --manifest-path verify/Cargo.toml --target-dir target/verus

# Apply formatters without changing program behavior.
fix:
    cargo fmt --all
    taplo format
    nixfmt flake.nix nix/verus.nix
