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
    nixfmt --check flake.nix
    quest check

# Run unit, doc, and trybuild UI tests across the workspace, aborting on any host-clock timer.
test *args:
    KIO_DENY_HOST_TIMERS=1 cargo test --locked --workspace {{ args }}
    KIO_DENY_HOST_TIMERS=1 cargo test --locked --doc -p kio --all-features

# Run what CI runs: lint, then test.
ci: check test

# Apply formatters without changing program behavior.
fix:
    cargo fmt --all
    taplo format
    nixfmt flake.nix
