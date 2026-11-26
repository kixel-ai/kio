# [S] Drop kio's MoQ names and unused public API

## Goal

kio's public surface names nothing from MoQ and carries no item without a user, landed as one breaking change before the first release from this repository, with `MIGRATE.md` and `CHANGELOG.md` saying how `moq-dev/moq` migrates.

## Plan

The 2026-11-26 code audit found these; each is public, so each waits on the maintainer:

- `Wall::MOQ_EPOCH` (`src/clock/wall.rs`): MoQ's 2020-01-01 media epoch. Its only user is `moq-dev/moq`'s `rs/hang/src/snapshot.rs`. Remove it and make `Wall::from_unix_secs` (today `pub(crate) const fn`) public, so hang defines `const EPOCH: Wall = Wall::from_unix_secs(1_577_836_800)` itself.
- `MOQ_SIM_SEED` (`src/clock/sim.rs`): the simulator's replay seed variable, beside `KIO_DENY_HOST_TIMERS`. Rename it to `KIO_SIM_SEED` with no alias, and update [adversarial-sim](/quest/m9/kio-state-machines/adversarial-sim.md), which names it. Nothing in `moq-dev/moq` sets it outside its docs (`doc/lib/rs/kio.md`).
- `kio::wake` (`src/waiter.rs`): wakes one waker under a `Hold`. Only its own test calls it, here or in `moq-dev/moq`. Remove it and the `Held::owned` list it alone fills.
- `ProducerWeak::poll_held_changed` (`src/weak.rs`): only `src/tests.rs` calls it, here or in `moq-dev/moq`. Remove it and `consumer::poll_held_changed`.

Checked and kept, since each is a natural part of its type's surface or has a downstream user: `Producer::poll_ref`, `Weak::consume_weak`, `log::Writer::SEGMENT_HEAP`, `Waiter::is_noop`, `Tasks::iter`, `PushError`, `Interval::period`, `Instant::elapsed`, `Wall::now`, `Installed::current`, `Installed::sleep_until`, and `sim::set_drift`.

## Waiting

- Maintainer: remove `Wall::MOQ_EPOCH` and publish `Wall::from_unix_secs` (recommended), keep `MOQ_EPOCH` as is, or remove it without a public constructor (hang then adds `Duration::from_secs(1_577_836_800)` to `Wall::UNIX_EPOCH` at runtime).
- Maintainer: rename `MOQ_SIM_SEED` to `KIO_SIM_SEED` with no alias (recommended), rename it but panic with a rename hint while `MOQ_SIM_SEED` is set, or keep it.
- Maintainer: remove `kio::wake` (recommended, no user) or keep it.
- Maintainer: remove `ProducerWeak::poll_held_changed` (recommended, no user) or keep it.

## Related

- [crates.io publishing](/quest/m1/crates-io-publish.md) - the first release from this repository, which these breaking changes should ride
