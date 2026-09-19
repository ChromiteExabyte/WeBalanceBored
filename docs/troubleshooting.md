# Windows troubleshooting

Work through the pipeline in order: Bluetooth, HID discovery, sensor values,
vJoy, then Steam. Stop at the first stage that fails.

## Build errors

| Message | Next step |
| --- | --- |
| `cargo` is not recognized | Install [Rust](https://rust-lang.org/tools/install/), then reopen PowerShell. |
| `Cargo.toml` could not be found | Change to the repository folder containing `Cargo.toml`. |
| `link.exe` not found | Install the Visual Studio Build Tools **Desktop development with C++** workload and a Windows SDK, then reopen PowerShell. |

## Windows says paired, but the app cannot find the board

Windows pairing records, PnP device status, and the HID interfaces available
to `hidapi` are different observations. A PnP status of `OK` is useful evidence,
but does not prove the app can open the device or receive reports. A Windows
friendly name also does not establish what `hidapi.product_string()` returns.

1. Press the front **Power** button to wake the paired board.
2. Run the diagnostic from the repository folder:

```pwsh
cargo run --release --locked -p balance-board-io --example list_hid_devices
```

The app searches for vendor `0x057E` and product `0x0306`, preferring a product
string containing `RVL-WBC-01`. It already allows a missing or generic product
string. These IDs are shared with Wii Remotes, so an ID match is a **candidate**,
not proof of a Balance Board.

| Diagnostic result | Next step |
| --- | --- |
| No matching VID/PID candidate | Check that the board is awake and inspect Windows' HID device entries below. Changing the product-name filter will not resolve an absent VID/PID match. |
| One candidate | Run `print_sensors` and capture its complete output. Opening, calibration, and live reports each establish a different stage of progress. |
| Multiple candidates | Disconnect other Wii Remotes/boards and retry. Automatic selection can pick the wrong device when names are generic. |

For comparison, this read-only PowerShell command lists relevant Windows devices:

```pwsh
Get-PnpDevice -PresentOnly |
    Where-Object {
        $_.FriendlyName -match 'Nintendo|RVL-WBC|Bluetooth HID' -or
        $_.InstanceId -match 'VID&0002057E_PID&0306|VID_057E&PID_0306'
    } |
    Select-Object Status, Class, FriendlyName, InstanceId |
    Format-List
```

If the board is absent from Windows too, revisit the
[connection steps](../README.md#2-connect-the-board). Red **SYNC** starts
pairing; flashing is not confirmation that pairing completed. Avoid repeatedly
removing a working pairing before collecting the HID diagnostic.

## Discovery succeeds, but calibration or streaming fails

The bridge now retries board discovery, calibration transport failures, and
sensor-read failures. No valid sensor data for three seconds counts as a
timeout, including when unrelated HID reports keep arriving. On a streaming
failure it clears vJoy input before waiting, then reopens the same HID path,
reads fresh calibration, and captures a new centered stance. Wake the board
with **Power**. Use **Ctrl+C** to stop retrying. If you re-pair the board and
Windows assigns a new HID path, restart the bridge to discover it again.

vJoy setup/output errors stop the bridge with an error instead of silently
continuing. The initial vJoy check also verifies the six required axes and
button 1, before waiting for the board.

Run the sensor example independently of vJoy:

```pwsh
cargo run --release --locked -p balance-board-io --example print_sensors
```

A register-read timeout or invalid calibration means discovery got further,
but the transport/handshake or protocol still needs investigation. Report the
exact error and HID candidate details; do not treat a later error as a fix.
The sensor example reads calibration directly from the board. The bridge can
also force a fresh read with `--no-cache`, which is needed after swapping boards
because the current cache is shared across devices.

Success is a continuously updating table whose loads respond to you stepping
on and leaning. Use **Ctrl+C** to stop a run that stalls, and include its last
printed line in the report.

## vJoy and Steam

| Symptom | Next step |
| --- | --- |
| `vJoyInterface.dll could not be loaded` | Confirm vJoy is installed and its DLL architecture matches the Rust executable. Add the installed DLL directory to this PowerShell session's PATH (example below). |
| `could not acquire vJoy device 1` | Enable device 1 in Configure vJoy and close other feeders that may own it. Enable X/Y/Z/Rx/Ry/Rz and at least one button. |
| Waiting for tare | Step on the board and stand still while it averages your stance. |
| `Streaming`, but no controller movement | Run with `--verbose`; check whether sensor values change, then inspect vJoy in `joy.cpl`. Streaming alone does not confirm output delivery. |
| vJoy moves in `joy.cpl`, but Steam does not see it | Continue with the [Steam guide](steam-input/superflight.md). Capture your Steam/vJoy versions; this integration is not yet verified end to end. |

For the common x64 vJoy install location, this changes PATH only in the current
terminal. Adjust the directory if your installation is elsewhere:

```pwsh
$vjoyDir = Join-Path $env:ProgramFiles 'vJoy\x64'
if (-not (Test-Path -LiteralPath (Join-Path $vjoyDir 'vJoyInterface.dll'))) {
    throw "vJoyInterface.dll was not found in $vjoyDir. Check your install location."
}
$env:Path = "$vjoyDir;$env:Path"
cargo run --release --locked -p balance-board-bridge -- --verbose
```

## A useful bug report

Include the exact command and full error, `git rev-parse --short HEAD` (for a
Git checkout), `rustc --version`, Windows version, Bluetooth adapter model,
whether the board was awake, and the Nintendo entries from `list_hid_devices`.
Add vJoy/Steam versions only if the failure reaches those stages. Device paths
and serial fields may contain Bluetooth addresses; redact those before posting
publicly if desired, retaining VID/PID, product strings, and usage fields.

Open a [bug report](https://github.com/ChromiteExabyte/WeBalanceBored/issues/new?template=bug_report.md)
with those details. See [CONTRIBUTING.md](../CONTRIBUTING.md) for development checks.
