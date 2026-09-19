# WeBalanceBored

A simple terminal launcher for the Wii Balance Board on **Windows and Linux**.
Connect the board, see live weight, or use your lean as a game controller.

**Experimental:** software tests pass independently of hardware. Pairing,
Bluetooth-adapter compatibility, and the complete Steam/game path still need
physical-board verification on each platform.

## Start here — no Rust required

Download the package for your OS from the latest successful
[Downloadable apps workflow](https://github.com/ChromiteExabyte/WeBalanceBored/actions/workflows/packages.yml).
Open a successful run and download its artifact (GitHub sign-in may be required).
Extract the outer artifact ZIP and then the app ZIP/tar.gz inside it. These are
CI builds, not signed installers or stable releases.

| Windows 11 | Linux desktop (x86-64) |
| --- | --- |
| Double-click **Start-WeBalanceBored.cmd**. | Run **setup-linux.sh** once, then **start-we-balance-bored.sh**. |
| Bluetooth is enough for live weight. | Needs BlueZ, libudev, udev/logind, and the kernel hid_wiimote driver. |
| Game mode additionally needs vJoy. | Game mode uses the kernel uinput interface; no vJoy. |

The menu is the same on both platforms:

```text
1  Connect and view live weight
2  Play — start controller output
3  Pair a board
4  Check setup
Q  Quit
```

For a new board, choose **3**, press red **SYNC** inside the battery compartment
when prompted, and complete pairing. Then choose **1** to verify that weight
changes when you step on the board. Flashing lights or a successful pairing
command alone do not prove sensor input works.

For daily use, wake the paired board with its front **Power** button and choose
**1** or **2**. **Ctrl+C** disconnects and clears controller output. A lost
connection is retried automatically. Keep only the intended board connected
when starting; reconnection stays with the selected device.

To zero an unloaded reading, **leave the board empty and tap/release the front
Power button while connected**. Keep it empty until **Weight zeroed** appears
(about 1.5 seconds after release). This averages a fresh baseline for each
corner on Windows and Linux. Repeat whenever needed; reconnecting or restarting
clears the baseline. The button is reserved for zeroing, including in controller
mode, where output pauses during zeroing and your stance is measured again.

### Windows game setup — once

Install [vJoy](https://github.com/jshafer817/vJoy/releases). In **Configure vJoy**,
enable device **1**, axes **X, Y, Z, Rx, Ry, Rz**, and at least **one button**.
The app searches standard vJoy installation folders automatically.
Choose **2**, stand comfortably centered, and wait for live readings. Check
axis movement in **joy.cpl**, then configure your game in Steam Input.

### Linux setup — once

On Debian/Ubuntu, install runtime prerequisites:

```sh
sudo apt install bluez libudev1
./setup-linux.sh
./start-we-balance-bored.sh
```

The setup script uses sudo to install device access rules and load kernel
modules. Run the launcher as your normal desktop user. It does not require
world-writable input devices or membership in the broad `input` group.
The packaged build targets x86-64 Linux with glibc 2.35 or newer; other
architectures can build from source. See [Linux help](docs/linux.md).

## Developer setup

Users of the downloadable packages do **not** need a compiler. To build locally,
install Rust and the platform build prerequisites:

- Windows: Visual Studio C++ Build Tools and a Windows SDK.
- Debian/Ubuntu: `sudo apt install build-essential pkg-config libudev-dev`.

```sh
cargo test --workspace --locked
cargo build --release --workspace --locked
cargo run --release --locked -p balance-board-bridge --bin we-balance-bored
```

From source, the Linux setup script is `packaging/linux/setup-linux.sh`.
The Windows pairing helper is built alongside the launcher by the workspace build.

## Direct commands

Run these built executables from `target/release` (append `.exe` on Windows),
or from the extracted package:

| Task | Command |
| --- | --- |
| Menu | `we-balance-bored` |
| Live weight, no game driver | `we-balance-bored --monitor` |
| Controller mode | `we-balance-bored --gamepad` |
| Bluetooth pairing | `we-balance-bored --pair` |
| Setup diagnostics | `we-balance-bored --doctor` |
| Legacy bridge entry point | `balance-board-bridge --verbose` |

Controller mode supports `--no-tare`, `--no-smooth`, and `--no-cache`.
Windows calibration is cached per HID path; reconnects always read it fresh.
Linux uses already-calibrated kernel readings, so it does not use that cache.
On Windows, restart the app after re-pairing if the HID path changes. Linux
reconnects by the board's unique identity when the kernel provides one.

For Windows diagnostics see [troubleshooting](docs/troubleshooting.md).
For game mapping see the [Superflight checklist](docs/steam-input/superflight.md).

## Workspace layout

| Crate | License | Purpose |
| --- | --- | --- |
| `balance-board-protocol` | MPL-2.0 | Pure parsing, calibration, center-of-gravity math, smoothing filter. No I/O, zero deps, runs on any machine without a board. |
| `balance-board-io` | MPL-2.0 | HID glue via `hidapi`. Reads bytes off the wire, hands them to the protocol crate. Cross-platform. |
| `balance-board-bridge` | GPL-3.0-or-later | Shared connection engine, terminal launcher, Windows vJoy and Linux uinput output, tare and smoothing. |
| `balance-board-pair` | GPL-3.0-or-later | Experimental Windows Bluetooth scan, pairing, and HID-service setup tool. |

The split licensing is deliberate: the reusable crates use file-level copyleft
(MPL-2.0) so anyone can pull them into their own projects; the bridge binary
is GPL-3.0 to keep derivative end-user tools open.

## Goals

1. Play Superflight (and other Steam games) using a Wii Balance Board, via
   the path `Balance Board → Bluetooth → bridge → virtual controller → Steam Input → game`.
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
