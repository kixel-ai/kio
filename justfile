set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available development commands.
default:
    @just --list

# Lint the workspace, feature matrix, workflows, TOML, Nix, and quest tree.
check:
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo clippy --locked -p kio --no-default-features -- -D warnings
    cargo clippy --locked -p kio --all-features --all-targets -- -D warnings
    cargo fmt --all --check
    actionlint
    taplo format --check
    nixfmt --check flake.nix nix/verus.nix
    quest check

# Run unit, doc, and trybuild UI tests across the workspace.
test *args:
    cargo test --locked --workspace {{ args }}
    cargo test --locked --doc -p kio --all-features

# Run what CI runs: lint, then test.
ci: check test

# Prove the `verus!` code in src/log.rs. Needs Verus: run it in `nix develop .#verus`.
verify:
    cargo verus verify -p kio

# Apply formatters without changing program behavior.
fix:
    cargo fmt --all
    taplo format
    nixfmt flake.nix nix/verus.nix
