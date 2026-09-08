# WeBalanceBored

[![CI](https://github.com/ChromiteExabyte/WeBalanceBored/actions/workflows/ci.yml/badge.svg)](https://github.com/ChromiteExabyte/WeBalanceBored/actions/workflows/ci.yml)

A Wii Balance Board → Steam Input bridge for Windows. Built as a Rust workspace
so the protocol parsing and calibration math are reusable by other Balance
Board projects, not locked inside this app.

## Status

**Pre-alpha: buildable from source, with hardware verification still in progress.**
Passing tests checks the software; it does not confirm that pairing, sensor
streaming, vJoy, and Steam work together on your machine.

| Layer | State |
| --- | --- |
| Protocol parsing & calibration | Implemented, unit-tested with byte fixtures |
| HID I/O (`hidapi`) | Implemented; needs hardware to verify |
| vJoy output (runtime LoadLibraryW FFI) | Implemented; needs vJoy + hardware to verify |
| Bridge with tare + smoothing + calibration cache | Implemented; full game pipeline not yet hardware-verified |
| Auto-pair tool (Win32 Bluetooth) | Scan implemented + verified; pair implemented, needs a SYNC-pressed board to fully verify |
| Steam Input setup guide for Superflight | [docs/steam-input/superflight.md](docs/steam-input/superflight.md) |
| System tray / config UI | Not started |

## Start here (Windows)

### 1. Build the software

Install [Rust](https://rust-lang.org/tools/install/) and the Visual Studio
C++ Build Tools when prompted. Reopen PowerShell after installing them.
Open PowerShell in the downloaded repository folder, the one containing
`Cargo.toml`, and run each command separately:

```pwsh
cargo test --workspace --locked
cargo build --release --workspace --locked
```

The first command tests the code without a board. The second compiles the
programs into `target\release`. Stop and resolve any error before continuing.

### 2. Connect the board

| Button | When to use it |
| --- | --- |
| Red **SYNC**, inside the battery compartment | Start pairing; the blue indicator flashes. Flashing alone does not confirm a connection. |
| Front **Power** button | Wake a board that has already been paired. |

If Windows already lists `Nintendo RVL-WBC-01`, wake the board and try the
sensor test below first. If it is not paired, press red **SYNC** immediately
before running the already-built pairing tool:

```pwsh
.\target\release\balance-board-pair.exe
```

This experimental tool attempts pairing and enables the HID service. Its
Bluetooth scan has been hardware-tested; the full pairing handshake still
needs verification. `--scan` lists nearby Wii devices without pairing them.
`--forget` removes **all** Balance Boards known to Windows; it is not a
routine startup step.

### 3. Check live sensor values

```pwsh
cargo run --release --locked -p balance-board-io --example print_sensors
```

This needs the board and Bluetooth, but **does not need vJoy or Steam**.
Success means a live table of corner loads and total kilograms that changes
when you step on the board. Press **Ctrl+C** to stop.

If it cannot find the board or read calibration, use the
[troubleshooting guide](docs/troubleshooting.md) before moving on.

### 4. Start the game-controller bridge

Install [vJoy](https://github.com/jshafer817/vJoy/releases) and open
**Configure vJoy**. Enable device **1**, axes **X, Y, Z, Rx, Ry, Rz**, and
at least **one button**. Then run:

```pwsh
cargo run --release --locked -p balance-board-bridge -- --verbose
```

When prompted, step on the board and stand comfortably still while the
bridge captures your centered stance (tare). Wait for `Streaming` and keep
this terminal running. Open `joy.cpl` from Windows Run (**Win+R**), select
vJoy, and check that its X/Y axes move when you lean.

Once that works, follow the [Superflight setup guide](docs/steam-input/superflight.md).
Steam recognition and the game mapping are separate checks; a successful
build or `Streaming` message alone does not prove that they work.

## Useful commands

Run these from the repository folder:

| Task | Command |
| --- | --- |
| Test protocol math without hardware | `cargo test --locked -p balance-board-protocol` |
| List the HID devices the app can actually see | `cargo run --release --locked -p balance-board-io --example list_hid_devices` |
| Scan Bluetooth without pairing | `.\target\release\balance-board-pair.exe --scan` |
| Show bridge options | `.\target\release\balance-board-bridge.exe --help` |
| Refresh calibration after switching boards | `.\target\release\balance-board-bridge.exe --no-cache --verbose` |

The bridge captures tare and smooths input by default. `--no-tare` and
`--no-smooth` disable those steps for diagnosis. Its calibration cache is a
single file at `%APPDATA%\WeBalanceBored\calibration.bin`, not keyed to the
board: use `--no-cache` once whenever you switch physical boards.

## Workspace layout

| Crate | License | Purpose |
| --- | --- | --- |
| `balance-board-protocol` | MPL-2.0 | Pure parsing, calibration, center-of-gravity math, smoothing filter. No I/O, zero deps, runs on any machine without a board. |
| `balance-board-io` | MPL-2.0 | HID glue via `hidapi`. Reads bytes off the wire, hands them to the protocol crate. Cross-platform. |
| `balance-board-bridge` | GPL-3.0-or-later | The end-user binary. vJoy output, tare + smoothing, calibration cache. |
| `balance-board-pair` | GPL-3.0-or-later | Experimental Windows Bluetooth scan, pairing, and HID-service setup tool. |

The split licensing is deliberate: the reusable crates use file-level copyleft
(MPL-2.0) so anyone can pull them into their own projects; the bridge binary
is GPL-3.0 to keep derivative end-user tools open.

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
- `balance-board-bridge`, `balance-board-pair` — GPL-3.0-or-later

For development checks and hardware test reports, see [CONTRIBUTING.md](CONTRIBUTING.md).
