# Changelog

All notable changes to this project. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once it leaves the 0.x series.

## [Unreleased]

### Added
- `balance-board-pair` crate — Windows auto-pair tool that scans for
  `Nintendo RVL-WBC-01` devices, computes the special Wii PIN
  (BD_ADDR as raw bytes), authenticates them via the Win32 Bluetooth
  API, and enables the HID service. Three subcommands: default
  (scan + pair + enable), `--scan`, `--forget`.
- Tare offset capture in the bridge: the user's natural-stance COG is
  averaged over a ~1 second warm-up and subtracted from subsequent
  readings so a relaxed centered stand reads as `(0, 0)`. Skippable
  via `--no-tare`.
- Two-axis exponential moving average (`LowPass2D`) in the protocol
  crate; the bridge applies it to COG before pushing to vJoy.
  Skippable via `--no-smooth`.
- Calibration cache at `%APPDATA%\WeBalanceBored\calibration.bin`.
  Subsequent launches skip the multi-frame EEPROM read. Validated
  via the protocol crate's monotonicity check; falls back to live
  read if the cache is corrupt or invalid. Skippable via `--no-cache`.
- `list_hid_devices` example in `balance-board-io` — diagnostic that
  dumps every HID device hidapi can see. Helps users understand
  hidapi-vs-Windows discovery quirks.
- Steam Input setup guide for Superflight at
  [docs/steam-input/superflight.md](docs/steam-input/superflight.md).
- GitHub Actions CI: Ubuntu-only protocol-crate tests + clippy, and
  Windows full-workspace build + tests + clippy. Cached via
  `Swatinem/rust-cache`.
- This `CHANGELOG.md` and `CONTRIBUTING.md`.

### Changed
- HID discovery is no longer strict-AND on the product string. After a
  Bluetooth pairing, hidapi on Windows often reports a generic
  `HID-compliant game controller` string for the child object;
  discovery now matches by VID + PID (Nintendo + 0x0306) and prefers
  a `RVL-WBC-01` product string when available. The error path
  references the new `list_hid_devices` example.
- `balance-board-bridge`'s vJoy FFI changed from compile-time
  `raw-dylib` import (which aborted with `STATUS_DLL_NOT_FOUND` on
  machines without vJoy installed) to runtime `LoadLibraryW` +
  `GetProcAddress`. The binary now builds and runs anywhere; vJoy is
  only required when actually acquiring a device.

### Verified
- 43 unit tests + 2 doc tests across the workspace, all green.
- Auto-pair tool's `--scan` mode confirmed to enumerate real Balance
  Boards on Windows (matched address against a known device's
  BTHENUM PnP ID).

### Not yet hardware-verified
- The pairing handshake itself (PIN delivery via `BluetoothSendAuthenticationResponseEx`).
- The full bridge end-to-end pipeline (board → vJoy → Steam Input → game).

<!-- last touched: 2026-05-12 -->
