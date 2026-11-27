set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available development commands.
default:
    @just --list

# Lint the workspace, feature matrix, unused dependencies, rustdoc, workflows, TOML, Nix, and quest tree.
check: coroutines
    cargo shear --locked --deny-warnings
    cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
    cargo clippy --locked -p kio --no-default-features -- -D warnings
    cargo fmt --all --check
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --workspace --all-features
    actionlint
    taplo format --check
    nixfmt --check flake.nix nix/verus.nix
    quest check

# Fail if kio without its `async` feature compiles a coroutine: an `async fn` body, `async` block, or `async` closure.
coroutines:
    #!/usr/bin/env bash
    set -euo pipefail
    # A stable path keeps rustc's args unchanged, so an unchanged rerun reuses the build
    # and the MIR it left behind instead of adding a build to the cache.
    mkdir -p target/coroutines
    # The bare default build, plus the features a poll-only consumer enables; `runtime`
    # swaps out the host-clock fallback, so neither build covers the other.
    for features in default macros,runtime,manual,serde; do
        # `--emit` splits on commas, so the file name cannot hold them.
        mir="target/coroutines/${features//,/+}.mir"
        # Only the final crate gets `--emit`, so dependencies build as usual.
        cargo rustc --locked -p kio --lib --no-default-features --features "${features#default}" -- --emit "mir=$mir"
        if [[ ! -s "$mir" ]]; then
            echo "error: no MIR at $mir; run \`cargo clean -p kio\` to rebuild it" >&2
            exit 1
        fi
        # rustc names each coroutine by its body, as `{async fn body of queue::Queue<T>::pop()}`.
        # Match the prefix only: a nested name holds braces, as `f::{closure#0}::g()`.
        found=$(grep -oE '\{async (fn body of |block@|closure@)[^ ;,]*' "$mir" | sed -E 's/\}[^}]*$/}/' | sort -u || true)
        if [[ -n "$found" ]]; then
            echo "error: kio --features '$features' compiles coroutines without its async feature:" >&2
            echo "$found" >&2
            echo "Gate each behind the feature, or generate it from a poll with #[kio::wrapper]." >&2
            exit 1
        fi
        echo "kio --features '$features': no coroutines"
    done

# Run unit, doc, and trybuild UI tests across the workspace, failing on any host-clock timer.
test *args:
    KIO_DENY_HOST_TIMERS=1 cargo test --locked --workspace --all-features {{ args }}

# Run kio's suite under each adversarial sim schedule with the lost-wakeup probe, about
# 0.4 s a pass. Each run draws a fresh seed and prints it on failure, as `test` does.
sweep:
    for schedule in random pct; do \
        KIO_DENY_HOST_TIMERS=1 KIO_SIM_PROBE=1 KIO_SIM_SCHEDULE=$schedule cargo test --locked -q -p kio --all-features; \
    done

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

# Run what CI runs: lint, test, then the adversarial sweep.
ci: check test sweep

# Fail a breaking PR title (`type!:` or `type(scope)!:`) whose diff from `base` changes no MIGRATE.md.
[positional-arguments]
migrate-check title base="origin/main":
    #!/usr/bin/env bash
    set -euo pipefail
    # Positional, so a hostile title is never spliced into the script.
    title=$1 base=$2
    if ! [[ $title =~ ^[A-Za-z]+(\([^\)]*\))?!: ]]; then
        echo "not breaking: $title"
        exit 0
    fi
    changed=$(git diff --name-only "$base...HEAD")
    if grep -qxE '(macros/)?MIGRATE\.md' <<<"$changed"; then
        echo "breaking, and MIGRATE.md changed: $title"
        exit 0
    fi
    echo "error: '$title' marks a breaking change, but the diff from $base changes neither MIGRATE.md nor macros/MIGRATE.md" >&2
    exit 1

# Prove src/log.rs's Verus proof. Needs Verus: run it in `nix develop .#verus`. The
# `verify` package builds kio's sources with `vstd`, which kio itself never depends on.
verify:
    cargo verus verify --locked --manifest-path verify/Cargo.toml --target-dir target/verus

# Apply formatters without changing program behavior.
fix:
    cargo fmt --all
    taplo format
    nixfmt flake.nix nix/verus.nix
