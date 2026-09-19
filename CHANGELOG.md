# Changelog

All notable changes to this project. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once it leaves the 0.x series.

## [Unreleased]

### Calibration response correction
- Fixed reversed size/error nibbles in Windows register-read responses. A
  successful 16-byte calibration chunk was incorrectly reported as error 0xF,
  preventing live weight after successful pairing.
- Added literal wire-format regression fixtures for the 24-byte calibration
  response and corrected the error-response fixture.
- Verified calibration and continuous live sensor readings on the paired
  Windows board. Loaded weight accuracy and controller output remain unverified.

### Windows SYNC pairing correction
- Replaced the callback-based authentication path with direct legacy PIN
  authentication using the local radio address for red SYNC, following the
  approach used by Dolphin and BlueZ. The previous device-address PIN was
  not appropriate for this pairing mode.
- Discovery and authentication now use the same radio, followed by a service
  inquiry and HID activation. Timeout 258 gets a specific diagnostic rather
  than being treated as evidence of a rejected PIN.
- Added tests for wide PIN encoding, embedded zero/high bytes, and timeout
  classification. Windows pairing and HID activation succeeded on the user's
  physical board; live sensor streaming also passed after the parser correction.

### Windows and Linux terminal launcher
- Added `we-balance-bored`: menu options for live weight, controller mode,
  pairing, and setup checks. Monitor mode needs no virtual-controller driver.
- Shared connection engine with cooperative Ctrl+C cleanup, per-device Windows
  calibration caching, and fresh stance capture after reconnect.
- Native Linux calibrated evdev input through hid_wiimote and uinput controller
  output. BlueZ pairing is available from the launcher.
- Standard vJoy installation directories are searched automatically.
- Linux one-time device-permission setup and packaged Windows/Linux launchers.
- Full-workspace Linux CI and downloadable ZIP/tar.gz build artifacts. These
  remain experimental builds; hardware and game integration are unverified.

### Connection reliability
- Pairing refreshes the Windows device record before checking authentication,
  avoiding unnecessary authentication of an already-paired board. Corrected
  contradictory PIN diagnostics; the existing PIN algorithm is unchanged.
- Sensor reads time out after three seconds without a valid sensor report.
- The bridge waits for the board and retries connection failures. It clears
  vJoy input on a sensor failure, reconnects to the same HID path, and rereads
  calibration and tare. Physical-board recovery testing remains outstanding.
- vJoy setup, axis, and button failures now produce errors. Acquisition checks
  the required axes/button, and normal release resets controller state.

### Added
- Windows first-run walkthrough and `docs/troubleshooting.md`, with separate
  checks for pairing, HID discovery, live sensors, vJoy, and Steam.
- Connection diagnostics fields in the bug-report template.
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
- Discovery errors now return `NotFound` when no VID/PID candidate is visible,
  and explain the existing generic-name fallback without assuming a pairing
  failure. Multi-device warnings identify the selected path and product string.
- HID diagnostics distinguish matching Wii VID/PID candidates from other
  Nintendo devices without treating a candidate as a confirmed Balance Board.
- Superflight instructions now mark the mapping as unverified and require
  vJoy/Steam checks before game setup; contributing instructions match CI's
  formatting checks.
- HID discovery matches by VID + PID (Nintendo + 0x0306), prefers a
  `RVL-WBC-01` product string when available, and allows a missing or
  generic product string. The error path references `list_hid_devices`
  so Windows PnP names need not be used to infer hidapi's values.
- `balance-board-bridge`'s vJoy FFI changed from compile-time
  `raw-dylib` import (which aborted with `STATUS_DLL_NOT_FOUND` on
  machines without vJoy installed) to runtime `LoadLibraryW` +
  `GetProcAddress`. The binary now builds and runs anywhere; vJoy is
  only required when actually acquiring a device.

### Verified
- Hardware-independent unit and doc tests cover protocol math, parsing,
  report assembly, PIN formatting, bridge processing, and calibration caching.
- Auto-pair tool's `--scan` mode confirmed to enumerate real Balance
  Boards on Windows (matched address against a known device's
  BTHENUM PnP ID).

### Not yet hardware-verified
- The revised Windows SYNC pairing handshake (direct `BluetoothAuthenticateDevice`).
- The full bridge end-to-end pipeline (board → vJoy → Steam Input → game).

<!-- last touched: 2026-05-13 -->
