# Migrating kio

How to update your code for each breaking change, newest first.

## Unreleased

Everything below breaks code written against `0.6.4`, the last release.

### `Hold` holds every wake on the thread ([moq-dev/moq#8293](https://github.com/moq-dev/moq/pull/8293))

`kio::hold()` returns a `Hold` that holds back every wake this thread makes through a
`WaiterList` until it drops; holds nest, and the outermost delivers. It takes no
generic parameter and is neither `Send` nor `Sync`. It replaces `Fan::hold()` and
`Hold<T>`: open a hold before taking the locks whose guards wake waiters, and let it
drop after them.

### `async` methods moved behind the `async` feature ([moq-dev/moq#7506](https://github.com/moq-dev/moq/pull/7506))

`Queue::{push, pop}`, `Producer::{wait, closed, wanted_changed}`, `Consumer::{wait,
closed}`, `Shared::wait`, `ConsumerWeak::closed`, `ProducerWeak::closed`,
`clock::drive`, and `clock::Interval::tick` now compile only with the new `async`
feature, which is off by default; `sim` enables it. Add `features = ["async"]` to
the `kio` dependency of each crate that `.await`s them. Each `poll_*` method is
unchanged, and still drives the same wait without it.

### Used and unused renamed to wanted ([moq-dev/moq#7036](https://github.com/moq-dev/moq/pull/7036))

On `Producer` and `ProducerWeak`:

- `is_used()` is now `wanted()`.
- `Producer::write_unused()` is now `write_unwanted()`, returning `Unwanted`, whose
  `Used` variant is now `Wanted`.
- `used()`, `unused()`, `poll_used(waiter)`, and `poll_unused(waiter)` are replaced by
  `poll_wanted_changed(waiter, last)`, which resolves to the new `wanted()` once it
  differs from `last`, or `Err(Closed)`. `Producer::wanted_changed(last)` awaits it
  with the `async` feature. Wait for a consumer with `last = false`, and for the last
  one to go with `last = true`.

### `MaybeSend` removed ([moq-dev/moq#7475](https://github.com/moq-dev/moq/pull/7475))

kio no longer builds for WASM, so the `MaybeSend` alias is gone. Bound on `Send`.

### `Pollable` merged into `Task`, and unused surface removed ([moq-dev/moq#4156](https://github.com/moq-dev/moq/pull/4156))

`Fan` with its `Hold<T>`, `WeakLock`, `Pollable`, and the public `Lock::downgrade` are
gone.
`Task` gained an `Output` type and has a blanket impl for `FnMut(&Waiter) -> Poll<R>`.
`poll_used` and `poll_unused` return `Poll<Result<(), Closed>>` instead of
`Poll<Option<()>>`, before their rename above.
`Waiter::context()` replaces `Context::from_waker(waiter.waker())`.
Match `Poll::Ready(Err(kio::Closed))` where you matched `Poll::Ready(None)`.
