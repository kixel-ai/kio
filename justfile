set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available development commands.
default:
    @just --list

# Lint the workspace, feature matrix, unused dependencies, workflows, TOML, Nix, and quest tree.
check:
    cargo shear --locked --deny-warnings
    cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
    cargo clippy --locked -p kio --no-default-features -- -D warnings
    cargo clippy --locked -p kio-reactor-lab -- -D warnings
    cargo fmt --all --check
    actionlint
    taplo format --check
    nixfmt --check flake.nix nix/verus.nix
    quest check

# Run unit, doc, and trybuild UI tests across the workspace, failing on any host-clock timer.
test *args:
    KIO_DENY_HOST_TIMERS=1 cargo test --locked --workspace --all-features {{ args }}

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
