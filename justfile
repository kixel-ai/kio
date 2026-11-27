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
