# [S] Drop kio's MoQ names

## Goal

kio's public surface names nothing from MoQ, landed as one breaking change before the first release from this repository, with `MIGRATE.md` saying how `moq-dev/moq` migrates.

## Plan

The 2026-11-26 code audit found two; each is public, so each waits on the maintainer:

- `Wall::MOQ_EPOCH` (`src/clock/wall.rs`): MoQ's 2020-01-01 media epoch. Several `moq-dev/moq` crates call it (hang, moq-mux, moq-gst, moq-hls, and moq-net at the time of the audit); grep `moq-dev/moq` main for every caller when migrating. Remove it and make `Wall::from_unix_secs` (today `pub(crate) const fn`) public, so MoQ defines `const EPOCH: Wall = Wall::from_unix_secs(1_577_836_800)` once in its own lowest crate.
- `MOQ_SIM_SEED` (`src/clock/sim.rs`): the simulator's replay seed variable, beside `KIO_DENY_HOST_TIMERS`. Rename it to `KIO_SIM_SEED` with no alias, and update [adversarial-sim](/quest/m9/kio-state-machines/adversarial-sim.md), which names it. Grep `moq-dev/moq` for any script, workflow, or doc that sets it.

Checked and kept: public items this repository never calls have callers in `moq-dev/moq` main, such as `kio::wake` (`rs/moq-net/src/model/pace.rs`), `ProducerWeak::poll_held_changed` (`rs/moq-net/src/model/track.rs`), `Producer::poll_ref`, `Weak::consume_weak`, and `log::Writer::SEGMENT_HEAP`. Grep moq's main branch, not a local checkout, before calling one unused.

## Waiting

- Maintainer: remove `Wall::MOQ_EPOCH` and publish `Wall::from_unix_secs` (recommended), keep `MOQ_EPOCH` as is, or remove it without a public constructor (MoQ then adds `Duration::from_secs(1_577_836_800)` to `Wall::UNIX_EPOCH` at runtime).
- Maintainer: rename `MOQ_SIM_SEED` to `KIO_SIM_SEED` with no alias (recommended), rename it but panic with a rename hint while `MOQ_SIM_SEED` is set, or keep it.

## Related

- [crates.io publishing](/quest/m1/crates-io-publish.md) - the first release from this repository, which these breaking changes should ride
