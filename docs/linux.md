# Linux setup and troubleshooting

Use the extracted Linux package: `./setup-linux.sh` once, then
`./start-we-balance-bored.sh` as your normal desktop user. Source checkouts
keep these scripts in `packaging/linux/`; build the workspace before running
`target/release/we-balance-bored`.

## Requirements

- x86-64 Linux with glibc 2.35 or newer for the packaged build.
- BlueZ (`bluetoothctl`) and a working Bluetooth adapter.
- Kernel `hid_wiimote` and `uinput` modules.
- libudev runtime and a local desktop session using udev/logind access rules.
- Building from source also needs Rust, a C toolchain, pkg-config, and libudev headers.

Debian/Ubuntu runtime packages: `sudo apt install bluez libudev1`.
Other distributions use their own package names. Headless/SSH sessions and
containers may not receive local-seat `uaccess` permissions; those are not the
supported easy-setup path. Do not run the whole app as root to work around this.

## Pair and verify

1. Choose **3** in the launcher. Press red **SYNC** and Enter when prompted.
2. The launcher scans with BlueZ, selects only devices named
   `Nintendo RVL-WBC-01`, and asks you to select one if several are found.
3. It pairs an unpaired board, trusts it for reconnect, and requests connection.
4. Choose **1** and check that the corner readings and total weight change.
5. Choose **2** for game output and stand centered for stance capture. The
   controller is named **WeBalanceBored Controller**. Map it in Steam Input.

The board's nonstandard PIN is handled by BlueZ's
[autopair plugin](https://github.com/bluez/bluez/blob/master/plugins/autopair.c),
which explicitly recognizes the Balance Board. A distribution that disables
that plugin may fail pairing. The launcher does not weaken Bluetooth security
settings or remove existing pairing records. Pairing still needs hardware
verification with your adapter and BlueZ version.

## How Linux input differs

The app reads the kernel's calibrated `hid_wiimote` evdev interface. It does not
send a second raw-HID initialization sequence while that driver owns the board.
The four kernel axes are TR, BR, TL, BL in hundredths of a kilogram, as defined
by the [kernel driver](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-wiimote-modules.c).
The app polls current axis state because a motionless board need not emit new
evdev events. Removing the device causes read failures and reconnection.

Linux output uses `uinput` with X/Y lean, Z/Rx/Ry/Rz corner loads, and button 1.
Controller discovery and mappings in individual games remain unverified.

## Common failures

| Message or symptom | Next step |
| --- | --- |
| `bluetoothctl` missing | Install BlueZ. |
| Scan fails | Enable Bluetooth and check the BlueZ service and adapter. |
| PIN/pairing fails | Press red SYNC again; check the BlueZ autopair plugin. |
| Board absent after pairing | Wake it with Power; check `hid_wiimote` is available. |
| Board access denied | Run setup once, reconnect, and if needed log out/in. |
| Cannot open `/dev/uinput` | Run setup, then reopen the app as your desktop user. |
| Weight works but game does not | Check Steam sees the virtual controller and map its axes. |

`we-balance-bored --doctor` reports board and permission status. It attempts
to open the sensor device, but does not pair devices or create a controller.

## What setup changes

`setup-linux.sh` installs `/etc/udev/rules.d/70-we-balance-bored.rules`, creates
`/etc/modules-load.d/we-balance-bored.conf`, loads `uinput` and `hid_wiimote`,
and reloads udev rules. The rules grant the active desktop user access to the
Balance Board, the resulting virtual controller, and `uinput`. Access to
`uinput` allows creating virtual input devices. No blanket `chmod 666` or
access to unrelated physical keyboards is added.

To undo setup, remove those two named files as administrator, reload udev
rules, and reboot. This does not remove Bluetooth pairing records.
