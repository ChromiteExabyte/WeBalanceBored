# Superflight setup checklist

**Experimental: this game setup has not yet been verified end to end.**
There is no tested, importable `.vdf` profile in this repository yet. Use the
checks below to establish which stages work on your machine before tuning
the controls.

The intended path is:

```text
Balance Board -> Bluetooth HID -> bridge -> vJoy -> Steam Input -> Superflight
```

On Linux, choose **2** in the launcher and look for **WeBalanceBored Controller**
in Steam. See [Linux help](../linux.md). The vJoy checks below apply to Windows.

## 1. Confirm live input reaches vJoy

Complete the [Windows quickstart](../../README.md#start-here--no-rust-required), then run:

```pwsh
cargo run --release --locked -p balance-board-bridge --bin balance-board-bridge -- --verbose
```

Step on the board when prompted, stand still for tare, and keep the bridge
running after `Streaming` appears. Open Windows Run (**Win+R**), enter
`joy.cpl`, select the vJoy controller, and open its test view. Lean left,
right, forward, and back: the X/Y indicators should respond.

If they do not, stop here and follow [troubleshooting](../troubleshooting.md).
A running bridge alone does not establish that vJoy received its output.

## 2. Check Steam recognition

Open Steam's controller settings and look for the vJoy device. If Steam offers
support or setup for generic controllers, enable/configure that device. Labels
vary by Steam version; Xbox, PlayStation, and Switch options are not
interchangeable substitutes for generic-controller support.

Valve lists generic DirectInput gamepads among
[Steam Input's supported devices](https://partner.steamgames.com/doc/features/steam_controller/device).
That does not verify this particular vJoy configuration. If the device responds
in `joy.cpl` but is absent from Steam, report that result with your Steam and
vJoy versions before attempting game bindings.

## 3. Map and test the controls

Enable Steam Input for Superflight as needed and open its controller layout
with vJoy selected. Map the board's X/Y to the gamepad stick that actually
controls steering in the game. The earlier proposed right-stick mapping is
unverified; confirm the game's controls with a normal controller first.

| Bridge output | Meaning |
| --- | --- |
| X | Left/right lean, relative to the captured stance |
| Y | Forward/back lean, relative to the captured stance |
| Z, Rx, Ry, Rz | Loads at top-right, bottom-right, top-left, bottom-left |
| Button 1 | Board button state reported by the firmware |

Start with smoothing enabled. Test all four directions in-game and invert an
axis in the layout if necessary. Adjust the inner deadzone only enough to
remove drift, then tune the response to a comfortable lean range. Keep a
keyboard or normal controller available for menus and any unmapped actions.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Drifts while standing still | Restart the bridge and stand still during tare; then adjust the inner deadzone if needed. |
| Input feels twitchy | Ensure `--no-smooth` is not set. |
| Input saturates too early | Inspect the game's sensitivity and Steam Input response/deadzone settings. `MIN_TOTAL_KG` is an unloaded-board threshold; per-corner full scale affects Z/Rx/Ry/Rz, not X/Y sensitivity. |
| No input in-game, but `joy.cpl` works | Confirm Steam sees vJoy, the layout targets that device, and the selected stick controls steering. |
| Unexpected weights after switching boards | Run with `--no-cache` once to bypass the per-device Windows calibration cache. |

## Share a working setup

Once verified, contribute your exported layout with import instructions, axis
assignments, Steam/vJoy versions, and a short hardware test report. Describe
what you actually tested, including centering, all four lean directions, and
menu controls. See [CONTRIBUTING.md](../../CONTRIBUTING.md).
