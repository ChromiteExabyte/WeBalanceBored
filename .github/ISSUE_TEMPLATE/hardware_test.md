---
name: Hardware test report
about: You ran the bridge with a real Balance Board and want to share results
title: "[hardware] "
labels: hardware-test
---

Most of this project's code can't be unit-tested, so first-party
hardware reports are the best validation we have. Thanks for
running it!

## What you tried

- [ ] `balance-board-pair --scan` (just lists devices)
- [ ] `balance-board-pair` (full pair handshake)
- [ ] `print_sensors` example (HID + EEPROM, no vJoy)
- [ ] `balance-board-bridge` (full pipeline)
- [ ] Steam Input mapping for a specific game: `__________`

## What worked

<!-- For each step you ran, did the output look sensible?
- pair: did the board move from "paired=no" to "paired=yes"?
- print_sensors: did the kg numbers move sensibly when you leaned?
- bridge: did vJoy axes move when you leaned (check joy.cpl)?
- Steam Input: did the in-game stick respond to your weight shifts?
-->

## What didn't work

<!-- Be as specific as possible. Paste terminal output, screenshots,
joy.cpl screenshots, anything. -->

## Environment

- **OS / Windows build:**
- **Rust version:** (`rustc --version`)
- **Bluetooth radio brand/driver:**
- **vJoy version:**
- **Steam version:** (Steam → About Steam)
- **Board's BD_ADDR (Bluetooth MAC):** (so we can correlate firmware quirks across reports)

## Notes / wins / gotchas

<!-- Anything that would help the next person who tries this. -->
