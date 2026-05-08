# WeBalanceBored

[![CI](https://github.com/ChromiteExabyte/WeBalanceBored/actions/workflows/ci.yml/badge.svg)](https://github.com/ChromiteExabyte/WeBalanceBored/actions/workflows/ci.yml)

A Wii Balance Board → Steam Input bridge for Windows. Built as a Rust workspace
so the protocol parsing and calibration math are reusable by other Balance
Board projects, not locked inside this app.

## Status

Pre-alpha.

| Layer | State |
| --- | --- |
| Protocol parsing & calibration | Implemented, unit-tested with byte fixtures |
| HID I/O (`hidapi`) | Implemented; needs hardware to verify |
| vJoy output (raw-dylib FFI) | Implemented; needs vJoy + hardware to verify |
| End-to-end bridge binary | Implemented; runs the full pipeline |
| Steam Input profile | Not started |
| System tray / config UI | Not started |

## Workspace layout

| Crate | License | Purpose |
| --- | --- | --- |
| `balance-board-protocol` | MPL-2.0 | Pure parsing, calibration, center-of-gravity math. No I/O, zero deps, runs on any machine without a board. |
| `balance-board-io` | MPL-2.0 | HID + Bluetooth glue. Reads bytes off the wire, hands them to the protocol crate. Windows-first. |
| `balance-board-bridge` | GPL-3.0-or-later | The end-user binary. vJoy output, system tray, mapping config. |

The split licensing is deliberate: the reusable crates use file-level copyleft
(MPL-2.0) so anyone can pull them into their own projects; the bridge binary
is GPL-3.0 to keep derivative end-user tools open.

## Build & run

```pwsh
cargo test -p balance-board-protocol                          # no hardware needed
cargo build --release --workspace                              # everything
cargo run --release -p balance-board-io --example print_sensors  # smoke test (board + Bluetooth)
cargo run --release -p balance-board-bridge                    # full bridge (board + vJoy)
```

### Prerequisites

1. **Rust toolchain** — `winget install Rustlang.Rustup`, or grab `rustup-init.exe` from <https://rustup.rs>.
2. **Pair the Balance Board** via Windows Settings → Bluetooth. The board appears as `Nintendo RVL-WBC-01`. The PIN is the board's MAC address with the bytes reversed; recent Windows handles this automatically when you press SYNC inside the battery cover. Once paired the board shows up as an HID device.
3. **Install vJoy** for the bridge binary: <https://github.com/jshafer817/vJoy/releases>. Run **Configure vJoy** afterwards, ensure device #1 is enabled, and check at least axes X, Y, Z, Rx, Ry, Rz. (The smoke-test example does *not* need vJoy.)
4. **Steam Input mapping** — launch a game with controller support, open Steam's controller settings, and bind vJoy's X/Y to the in-game stick of your choice. For Superflight: vJoy X → right-stick X, vJoy Y → right-stick Y, plus a small radial deadzone.

## Goals

1. Play Superflight (and other Steam games) using a Wii Balance Board, via
   the path `Balance Board → Bluetooth HID → vJoy → Steam Input → game`.
   Step-by-step guide: [docs/steam-input/superflight.md](docs/steam-input/superflight.md).
2. Provide a clean, documented Rust crate that other Balance Board projects
   can depend on for parsing, calibration, and center-of-gravity math.

Inspired by, and rewritten from scratch over,
[lshachar/WiiBalanceWalker](https://github.com/lshachar/WiiBalanceWalker).

## License

This repository ships under two licenses depending on the crate.
Each crate's `Cargo.toml` declares its license via SPDX identifier; the
canonical license texts are at `LICENSE-MPL-2.0` and `LICENSE-GPL-3.0`.

- `balance-board-protocol`, `balance-board-io` — MPL-2.0
- `balance-board-bridge` — GPL-3.0-or-later
