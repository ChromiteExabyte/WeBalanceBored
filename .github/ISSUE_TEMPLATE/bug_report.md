---
name: Bug report
about: Something isn't working as expected
title: "[bug] "
labels: bug
---

## What's happening

<!-- A short description of the unexpected behavior. -->

## Reproduction

```pwsh
# The exact commands you ran, including any flags.
```

## What you expected

<!-- What you thought would happen instead. -->

## What you saw

<!-- The full terminal output. Use a code fence so formatting is preserved. -->

```text
…
```

## Environment

- **Crate / binary affected:** (e.g. `balance-board-bridge`, `balance-board-pair`, `print_sensors` example)
- **OS:** (Windows 10/11, version; or other)
- **Rust version:** (`rustc --version`)
- **Commit:** (`git rev-parse --short HEAD`, or ZIP download date)
- **Bluetooth adapter model (if relevant):**
- **vJoy version (if relevant):**
- **Steam version (if relevant):**
- **Board hardware:** Wii Balance Board (`Nintendo RVL-WBC-01`) — or another Wii-family device?

## Connection diagnostics (if relevant)

<!-- See docs/troubleshooting.md. Paste the Nintendo entries from:
cargo run --release --locked -p balance-board-io --example list_hid_devices
If there are no Nintendo entries, include the summary line.
Device paths and serial fields may contain Bluetooth addresses; you may redact
those, but retain VID/PID, product strings, and usage fields.
-->

- **Board awake or flashing during the test:**
- **Other Wii Remotes or boards connected:**
- **Last working stage:** Bluetooth discovery / HID discovery / live sensors / vJoy axes / game input

## Anything else

<!-- Screenshots, related logs, anything that helps reproduce or diagnose. -->
