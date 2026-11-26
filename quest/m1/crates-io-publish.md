# [XS] Configure crates.io trusted publishing for kio and kio-macros

## Goal

`kio-macros` and `kio` publish from `kixel-ai/kio`'s `release.yml` workflow via crates.io trusted publishing (`crates-io` GitHub environment, `main` branch only), and `kio-macros` `0.1.0` plus the next `kio` release are published so `moq-dev/moq` can switch from a git dependency back to crates.io before its next release cut.

## Waiting

- Maintainer: configure crates.io trusted publishing on https://crates.io for `kio` (and publish or reserve `kio-macros` `0.1.0`) trusting `kixel-ai/kio`, workflow `release.yml`, environment `crates-io`. An agent cannot change crates.io crate ownership or trusted-publisher settings without browser login. Verify by checking `crates.io/api/v1/crates/kio-macros` returns 200 and a `release.yml` run succeeds.

## Plan

`.github/workflows/release.yml` already packages `kio-macros` and `kio` in an unprivileged job and publishes `kio-macros` then `kio` using `rust-lang/crates-io-auth-action` inside the `crates-io` environment. Note that crates.io already holds `kio 0.6.4` from `moq`'s `release` branch, while `main` here carries unreleased changes past `0.6.2` (including `kio-macros`), so the first release from `kixel-ai/kio` should bump `kio` past `0.6.4` (e.g., `0.7.0` if breaking, or `0.6.5`).

Public API: none.
