# Folio's block 0.1.6 compatibility patch

Source: the crates.io `block` 0.1.6 archive, by Steven Sheldon, declared MIT in
its Cargo metadata. Upstream: <https://github.com/SSheldon/rust-block>.
The upstream archive includes no separate license file; `LICENSE` reproduces
the declared MIT terms with the author's attribution.

Folio's pinned wgpu 27 Metal backend and metal 0.32 both depend on this version.
The desktop manifest patches that dependency to this directory so offline
builds use the same fix without modifying Cargo's registry cache.

Runtime changes are limited to:

- Represent the opaque foreign `Class` as an inhabited `repr(C)` struct instead
  of an empty enum. Rust rejects uninhabited statics; see
  <https://github.com/rust-lang/rust/issues/74840>.
- Take the `_NSConcreteStackBlock` address with `ptr::addr_of!`, without creating
  a Rust reference to the foreign object's storage. Its bytes are never read.
- Spell out the existing `C` ABI on extern declarations and function types;
  implicit ABI declarations are deprecated by current Rust.

Public APIs, block layout, runtime symbols, and copying/releasing are unchanged.
The upstream unit-test source is preserved, but disabled because its
`objc_test_utils` path dependency is absent from the published archive. The
self-contained `tests/runtime.rs` instead checks calling, native heap copying,
reference counting, and exactly-once capture destruction on macOS. Cargo
metadata is reduced to the library and these tests, with publishing disabled.

`scripts/check.sh --desktop` compiles the patched dependency with future
incompatibilities denied, then runs its runtime tests and documentation examples.
Remove this patch when upgrading the GPU dependencies to versions that no longer
use the affected declaration. Do not suppress `uninhabited_static`.
