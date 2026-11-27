set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available development commands.
default:
    @just --list

# Lint the workspace, feature matrix, unused dependencies, rustdoc, workflows, TOML, Nix, and quest tree.
check:
    cargo shear --locked --deny-warnings
    cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
    cargo clippy --locked -p kio --no-default-features -- -D warnings
    cargo fmt --all --check
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --workspace --all-features
    actionlint
    taplo format --check
    nixfmt --check flake.nix nix/verus.nix
    quest check

# Run unit, doc, and trybuild UI tests across the workspace, failing on any host-clock timer,
# then kio's suite under each adversarial sim schedule with the lost-wakeup probe.
test *args:
    KIO_DENY_HOST_TIMERS=1 cargo test --locked --workspace --all-features {{ args }}
    # About 0.4 s a pass. Each run draws a fresh seed and prints it on failure, as the first pass does.
    for schedule in random pct; do \
        KIO_DENY_HOST_TIMERS=1 KIO_SIM_PROBE=1 KIO_SIM_SCHEDULE=$schedule cargo test --locked -q -p kio --all-features {{ args }}; \
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

# Run what CI runs: lint, then test.
ci: check test

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
