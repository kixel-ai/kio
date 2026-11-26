# [S] Drop kio's MoQ names

## Goal

kio's public surface names nothing from MoQ, landed as one breaking change before the first release from this repository, with `MIGRATE.md` saying how `moq-dev/moq` migrates.

## Plan

The 2026-11-26 code audit found two, both public:

- `Wall::MOQ_EPOCH` (`src/clock/wall.rs`): MoQ's 2020-01-01 media epoch. Several `moq-dev/moq` crates call it (hang, moq-mux, moq-gst, moq-hls, and moq-net at the time of the audit); grep `moq-dev/moq` main for every caller when migrating. Remove it and make `Wall::from_unix_secs` (today `pub(crate) const fn`) public, so MoQ defines `const EPOCH: Wall = Wall::from_unix_secs(1_577_836_800)` once in its own lowest crate.
- `MOQ_SIM_SEED` (`src/clock/sim.rs`): the simulator's replay seed variable, beside `KIO_DENY_HOST_TIMERS`. Rename it to `KIO_SIM_SEED` with no alias, and update [adversarial-sim](/quest/m9/kio-state-machines/adversarial-sim.md), which names it. Grep `moq-dev/moq` for any script, workflow, or doc that sets it.

Decided 2026-11-26 (maintainer, /quest-run): keep the quest.

Decided 2026-11-26 (maintainer, /quest-run): ✅ remove `Wall::MOQ_EPOCH` and make `Wall::from_unix_secs` public, so moq defines its own epoch (rejected: keep, rename).

Decided 2026-11-26 (maintainer, /quest-run): ✅ rename `MOQ_SIM_SEED` to `KIO_SIM_SEED` with no alias (rejected: alias, keep).

Checked and kept: public items this repository never calls have callers in `moq-dev/moq` main, such as `kio::wake` (`rs/moq-net/src/model/pace.rs`), `ProducerWeak::poll_held_changed` (`rs/moq-net/src/model/track.rs`), `Producer::poll_ref`, `Weak::consume_weak`, and `log::Writer::SEGMENT_HEAP`. Grep moq's main branch, not a local checkout, before calling one unused.

## Related

- [crates.io publishing](/quest/m1/crates-io-publish.md) - the first release from this repository, which these breaking changes should ride
