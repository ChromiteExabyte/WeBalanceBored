# Contributing to WeBalanceBored

Hi! If you have a Wii Balance Board and want to make this project better,
this page tells you how.

## Quickstart

```pwsh
git clone https://github.com/ChromiteExabyte/WeBalanceBored
cd WeBalanceBored
cargo test  --workspace          # all tests, no hardware needed
cargo build --release --workspace
```

Rust 1.75+ is the floor. On Windows you need the MSVC linker (Visual
Studio Build Tools "Desktop development with C++" workload, or VS
Community with the same workload). On Linux/macOS, the protocol crate
builds with stock cargo; the I/O and bridge crates need libudev / a
real hidapi backend respectively (those layers are Windows-first
today).

## Repository layout

| Crate / dir | What lives here |
| --- | --- |
| `crates/balance-board-protocol/` | Pure parsing, calibration, COG math, smoothing filter. Zero deps, MPL-2.0. **Most contributions to algorithms, formats, or reusable types belong here.** |
| `crates/balance-board-io/` | `hidapi` discovery + Wiimote handshake + EEPROM read assembler. MPL-2.0. |
| `crates/balance-board-bridge/` | The end-user binary. vJoy, tare, smoothing, calibration cache. GPL-3.0-or-later. |
| `crates/balance-board-pair/` | Windows auto-pair tool. GPL-3.0-or-later. |
| `docs/steam-input/` | Per-game Steam Input mapping recipes. Add yours! |
| `.github/workflows/ci.yml` | Tests + clippy on every push and PR. |

## Test policy

- Anything testable without hardware **should** have a unit test. The
  protocol crate is the gold standard — every byte-format claim is
  pinned by a fixture test.
- Hardware-dependent code (`hidapi_source`, `vjoy`, `bluetooth`) is
  intentionally not unit-tested. Manual verification is the contract;
  in PR descriptions, write what you tested on real hardware.
- Run `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
  before pushing.

## Style

- We follow stock `rustfmt` defaults; just running `cargo fmt` is
  fine. (Not enforced in CI yet — relax until we add a `fmt` job.)
- Every public item gets a doc comment. The `#![warn(missing_docs)]`
  in each crate's `lib.rs` will tell you when you forget.
- New `unsafe` blocks need a `// SAFETY:` comment explaining the
  invariant. The protocol crate has `#![forbid(unsafe_code)]` and
  should stay that way.
- Errors use `std::io::Error::other(...)` (not `Error::new(Other, ...)`,
  which clippy now flags).

## Licensing

We use a deliberate split:

- `balance-board-protocol`, `balance-board-io` are **MPL-2.0** —
  file-level copyleft, friendly to embed in other projects (including
  closed-source ones, with the constraint that modifications to *our*
  files stay open).
- `balance-board-bridge`, `balance-board-pair` are **GPL-3.0-or-later** —
  the binary that's the actual application of all this. If you fork
  and ship a derivative end-user tool, it's GPL too.

By submitting a PR you agree your contribution is licensed under the
SPDX identifier already declared in the affected crate's `Cargo.toml`.

## Good first contributions

If you're new and looking for somewhere to dig in:

- **Add a Steam Input recipe** under `docs/steam-input/` for a game
  that benefits from Balance Board input — racing, snowboarding,
  rhythm, fitness. The format is the existing `superflight.md`.
- **Improve the discovery error message** in
  `balance-board-io/src/hidapi_source.rs` if you hit a confusing
  failure mode and figured out what helps.
- **Fix a flaky test** if you hit one. The bridge's cache module
  uses tempdirs based on PID; if you find a race, raise it.
- **Verify pair_first on real hardware** — the auto-pair tool's
  scan path is verified, but the full pairing handshake hasn't been
  exercised end-to-end yet. If you run it and it works (or doesn't),
  open an issue with the result.

## What lives somewhere else

- **Bug reports / feature ideas** → GitHub Issues.
- **Steam Input questions** → see [docs/steam-input/superflight.md](docs/steam-input/superflight.md).
- **Architectural questions** → check the per-crate `lib.rs` doc
  comment first; each crate states its design intent.

Thanks for being here.
