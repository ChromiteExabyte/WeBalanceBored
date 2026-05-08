# balance-board-protocol

Pure parsing, calibration, and center-of-gravity math for the Nintendo Wii
Balance Board (Bluetooth HID device `Nintendo RVL-WBC-01`).

This crate does **no I/O**. Hand it bytes off the wire and the 24-byte EEPROM
calibration block; it returns typed sensor values, weight in kilograms per
sensor, and a normalized center-of-gravity reading. Every code path is
unit-tested with byte fixtures, so you can develop against this crate without
a real board.

```rust
use balance_board_protocol::{parse_report_extension, Calibration};

let ext = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
let raw = parse_report_extension(&ext);
let cal = Calibration::from_eeprom(&eeprom_bytes)?;
let kg  = cal.calibrate(raw);
let cog = kg.center_of_gravity(2.0);   // ignore COG below 2kg load
```

## Protocol references

- [WiiBrew: Wii Balance Board](https://wiibrew.org/wiki/Wii_Balance_Board)
- [WiiBrew: Wiimote/Extension Controllers](https://wiibrew.org/wiki/Wiimote/Extension_Controllers)

Sensor order on the wire (TR, BR, TL, BL) and the three calibration reference
loads (0 kg, 17 kg, 34 kg per sensor) are encoded as types in this crate so
you don't need to remember them.

## License

MPL-2.0. File-level copyleft — embed in any project, including proprietary;
modifications to this crate's files must remain MPL-2.0.
