# Playing Superflight with a Wii Balance Board

End-to-end recipe to get from a paired board to actually flying through
canyons. Assumes you already ran:

```pwsh
cargo run --release -p balance-board-bridge
```

and saw the bridge open the board, capture tare, and start streaming
("Streaming. Ctrl-C to stop.").

If you didn't get that far, see [the discovery diagnostic][list-devices]
or back up to the project [README][readme].

[list-devices]: ../../crates/balance-board-io/examples/list_hid_devices.rs
[readme]: ../../README.md

---

## What we're building

```
Balance Board → bridge → vJoy device #1 → Steam Input → Superflight
              \────── this repo ──────/  \─── Steam UI configures ───/
```

The bridge is doing the hard part already — turning your weight shifts
into a virtual gamepad. Steam Input is what tells Superflight *which*
gamepad axis means *which* in-game action.

---

## 1. Confirm Steam sees vJoy as a controller

While `balance-board-bridge` is running, lean around on the board. You
should already be feeding vJoy device #1.

In Steam:

1. **Steam → Settings → Controller**.
2. Enable **"Generic Gamepad Configuration Support"** (sometimes labeled
   "PlayStation / Switch / Xbox Extended Configuration Support" — the
   relevant toggle is whichever covers generic DirectInput devices).
3. Scroll to **Detected Controllers**. You should see something like
   `vJoy Virtual Joystick` or `Generic USB Joystick`. If you don't:
   - Re-check that `balance-board-bridge` is still running and the
     terminal shows `Streaming. Ctrl-C to stop.`
   - In Windows: open `joy.cpl` (Win+R, type `joy.cpl`). Pick the vJoy
     device, click **Properties → Test**, lean on the board. If the
     X/Y crosshair moves, vJoy is good. If not, the bridge isn't
     reaching vJoy — re-check vJoy install and that device #1 is
     enabled in vJoyConf.

---

## 2. Add Superflight (if not on Steam) and launch it

Superflight is on Steam, so just install it normally. Launch it once to
confirm it runs.

Quit back to Steam.

---

## 3. Bind vJoy axes to the in-game stick

1. In Steam, **right-click Superflight → Manage → Controller layout**
   (the wording shifts between Steam versions; the goal is the
   per-game controller configuration UI).
2. With Superflight's controller layout open, make sure the active
   controller is your vJoy device, not your real gamepad.
3. Click **Edit Layout** and pick a starting template — **Gamepad** is
   fine. We're overwriting the bits we care about.
4. **Right Stick → Click anywhere on the stick → Bind to Joystick**:
   - Joystick **X axis** → vJoy axis **X**
   - Joystick **Y axis** → vJoy axis **Y**
5. **Deadzones** (Right Stick → Settings):
   - Outer deadzone: ~0.05 (so leaning hard fully maxes the input)
   - Inner deadzone: ~0.10 (so a relaxed neutral stand stays still
     even after the bridge's tare)
   - Anti-deadzone: 0 (let the bridge handle that)

Save the layout (give it a name like "Balance Board"). Steam Input
applies it immediately while the game is running.

> The bridge also publishes per-corner kg loads on **Z, Rx, Ry, Rz**.
> Superflight doesn't need them, but you can bind them later for
> chord moves (e.g. heavy bottom-corner press → boost).

---

## 4. Sanity check before flying

In the controller config, with the binding view open, lean forward on
the board. The on-screen Right Stick indicator should move forward.
Same for back / left / right. If the directions feel inverted, swap by
toggling the axis "Invert" checkbox per-axis in the binding settings —
that's faster than re-running the bridge with a different sign convention.

---

## 5. Fly

Launch Superflight from Steam. Right Stick controls the plane. Lean
forward to dive, back to climb, side-to-side to bank.

> First-flight tip: stand close to the front of the board. Wii Balance
> Boards have a bit more sensor on the front edge than the back, and
> standing centered front-to-back makes the up/down range feel
> symmetric. The bridge's tare handles small offsets but can't fix a
> dramatic stance bias.

---

## Troubleshooting

| Symptom | Likely cause |
| --- | --- |
| Stick drifts when standing still | Re-run bridge so tare re-captures, or bump Steam Input inner deadzone to 0.15. |
| Stick feels twitchy / wobbly | Re-run without `--no-smooth`. Or lower `COG_ALPHA` constant in `main.rs` for more smoothing (slower response). |
| Stick maxes out before fully leaning | Lower `MIN_TOTAL_KG` (currently 2.0) or reduce Steam Input outer deadzone. Also consider shortening the bridge's per-corner full-scale: lean affects X/Y via COG which is normalized per-frame, so this shouldn't bite typically. |
| Steam doesn't see vJoy | Run `joy.cpl` test as above. If joy.cpl shows axes moving, the issue is Steam Input — toggle Generic Gamepad Support off/on, restart Steam. |
| Bridge prints "could not acquire vJoy device 1" | vJoy device 1 isn't enabled. Open **vJoyConf** (Start menu), tick device 1, ensure axes X/Y/Z/Rx/Ry/Rz are enabled, click Apply. |

---

## Other games

This same vJoy controller config works for any game that accepts a
generic DirectInput / Xbox-style gamepad through Steam Input. The
mapping is just: lean = stick movement. For non-flight games:

- **Driving / racing**: bind X to steering, Y to throttle/brake split.
- **First-person**: bind to *Left* Stick (movement) instead of Right
  Stick (camera) — leaning to walk around is more natural than
  leaning to look around.
- **Rhythm / fitness**: per-corner Z/Rx/Ry/Rz axes can detect each
  foot independently — useful for stomp-on-pad mechanics.

If you build a config for a specific game and it works well, send a PR
adding it to this directory.
