set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available development commands.
default:
    @just --list

# Lint the workspace, feature matrix, unused dependencies, workflows, TOML, Nix, and quest tree.
check:
    cargo shear --locked --deny-warnings
    cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
    cargo clippy --locked -p kio --no-default-features -- -D warnings
    cargo fmt --all --check
    actionlint
    taplo format --check
    nixfmt --check flake.nix nix/verus.nix
    quest check

# Run unit, doc, and trybuild UI tests across the workspace.
test *args:
    cargo test --locked --workspace --all-features {{ args }}

# Run what CI runs: lint, then test.
ci: check test

# Prove src/log.rs's Verus proof. Needs Verus: run it in `nix develop .#verus`. The cfg
# adds `vstd`, which plain builds never compile; its own target dir keeps the changed
# flags from rebuilding the plain one.
verify:
    RUSTFLAGS="--cfg verus_keep_ghost" cargo verus verify -p kio --target-dir target/verus

# Apply formatters without changing program behavior.
fix:
    cargo fmt --all
    taplo format
    nixfmt flake.nix nix/verus.nix
