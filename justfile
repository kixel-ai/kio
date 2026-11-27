set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available development commands.
default:
    @just --list

# Lint the workspace, feature matrix, unused dependencies, rustdoc, workflows, TOML, Nix, and quest tree.
check: coroutines
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

# Fail if kio without its `async` feature compiles a coroutine: an `async fn` body, `async` block, or `async` closure.
coroutines:
    #!/usr/bin/env bash
    set -euo pipefail
    mir=$(mktemp)
    trap 'rm -f "$mir"' EXIT
    # The bare default build, plus the features a poll-only consumer enables; `runtime`
    # swaps out the host-clock fallback, so neither build covers the other.
    for features in "" macros,runtime,manual,serde; do
        # Only the final crate gets `--emit`, so dependencies build as usual.
        cargo rustc --locked -p kio --lib --no-default-features --features "$features" -- --emit "mir=$mir"
        # rustc names each coroutine by its body, as `{async fn body of queue::Queue<T>::pop()}`.
        found=$(grep -oE '\{async (fn body of [^{}]*|block@[^{}]*|closure@[^{}]*)\}' "$mir" | sort -u || true)
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
