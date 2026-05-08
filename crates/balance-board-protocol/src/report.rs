//! Parsing Wiimote-style HID reports into raw Balance Board sensor values.
//!
//! The Balance Board is a Wiimote extension that piggybacks on the standard
//! Wiimote HID report stream. Different report IDs put the 8-byte extension
//! payload at different offsets; this module translates a few common ones
//! into [`RawSensors`].
//!
//! # References
//!
//! - WiiBrew Wiimote — Data Reporting Modes (0x32, 0x34, etc.):
//!   <https://wiibrew.org/wiki/Wiimote#Data_Reporting>
//! - WiiBrew Wii Balance Board — extension payload layout:
//!   <https://wiibrew.org/wiki/Wii_Balance_Board#Data_Format>

use crate::buttons::WiimoteButtons;
use crate::error::ParseError;
use crate::sensors::RawSensors;

/// Everything we extract from a Wiimote core-data HID report: the four
/// Balance Board sensors and the Wiimote button bitfield.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardReport {
    /// The four corner sensors (top-right, bottom-right, top-left,
    /// bottom-left), still as raw 16-bit values — calibration happens
    /// downstream.
    pub sensors: RawSensors,
    /// Wiimote button state from the report's first two payload bytes.
    /// On a Balance Board, the only physical button (front-edge SYNC)
    /// is exposed via [`WiimoteButtons::balance_board_button`].
    pub buttons: WiimoteButtons,
}

/// Wiimote/Balance Board HID report ID.
///
/// Only report IDs that include extension bytes are useful for the Balance
/// Board. The board sends its sensor data as the extension payload of
/// whichever data-reporting mode the host has selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportId {
    /// `0x32` — Core Buttons + 8 Extension. Typical mode for Balance Board work.
    CoreButtonsExt8,
    /// `0x34` — Core Buttons + 19 Extension. Larger frame; first 8 ext bytes are the board.
    CoreButtonsExt19,
}

impl ReportId {
    /// Offset (in bytes from start of report) where the 8-byte Balance Board
    /// payload begins.
    fn extension_offset(self) -> usize {
        // Both modes use 1 byte report ID + 2 bytes core buttons, then extension.
        match self {
            ReportId::CoreButtonsExt8 | ReportId::CoreButtonsExt19 => 3,
        }
    }

    fn min_len(self) -> usize {
        match self {
            ReportId::CoreButtonsExt8 => 1 + 2 + 8,
            ReportId::CoreButtonsExt19 => 1 + 2 + 19,
        }
    }

    fn from_byte(b: u8) -> Result<Self, ParseError> {
        match b {
            0x32 => Ok(ReportId::CoreButtonsExt8),
            0x34 => Ok(ReportId::CoreButtonsExt19),
            other => Err(ParseError::UnsupportedReportId(other)),
        }
    }
}

/// Parse the 8-byte Balance Board extension payload into raw sensor values.
///
/// The payload encodes four 16-bit big-endian sensor values in the order
/// top-right, bottom-right, top-left, bottom-left. This is the smallest
/// useful unit of parsing — if you've already extracted the extension bytes
/// (e.g. via a Wiimote library), call this directly.
#[must_use]
pub fn parse_report_extension(ext: &[u8; 8]) -> RawSensors {
    RawSensors {
        top_right: u16::from_be_bytes([ext[0], ext[1]]),
        bottom_right: u16::from_be_bytes([ext[2], ext[3]]),
        top_left: u16::from_be_bytes([ext[4], ext[5]]),
        bottom_left: u16::from_be_bytes([ext[6], ext[7]]),
    }
}

/// Parse a full Wiimote HID report into a [`BoardReport`] (sensors + buttons).
///
/// Supported report IDs: `0x32` (Core Buttons + 8 Extension) and `0x34`
/// (Core Buttons + 19 Extension). Returns [`ParseError`] for anything else.
pub fn parse_report(report: &[u8]) -> Result<BoardReport, ParseError> {
    let &id_byte = report.first().ok_or(ParseError::EmptyReport)?;
    let id = ReportId::from_byte(id_byte)?;
    if report.len() < id.min_len() {
        return Err(ParseError::TruncatedReport {
            expected: id.min_len(),
            got: report.len(),
        });
    }
    let buttons = WiimoteButtons::from_bytes(report[1], report[2]);
    let off = id.extension_offset();
    let ext: &[u8; 8] = report[off..off + 8]
        .try_into()
        .expect("bounds checked by min_len above");
    let sensors = parse_report_extension(ext);
    Ok(BoardReport { sensors, buttons })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_big_endian() {
        let ext = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let raw = parse_report_extension(&ext);
        assert_eq!(raw.top_right, 0x1234);
        assert_eq!(raw.bottom_right, 0x5678);
        assert_eq!(raw.top_left, 0x9ABC);
        assert_eq!(raw.bottom_left, 0xDEF0);
    }

    #[test]
    fn parses_full_0x32_report() {
        // 0x32 = report ID, 0x00 0x00 = core buttons, then 8 extension bytes.
        let mut report = vec![0x32, 0x00, 0x00];
        report.extend_from_slice(&[0x10, 0x00, 0x20, 0x00, 0x30, 0x00, 0x40, 0x00]);
        let parsed = parse_report(&report).unwrap();
        assert_eq!(parsed.sensors.top_right, 0x1000);
        assert_eq!(parsed.sensors.bottom_right, 0x2000);
        assert_eq!(parsed.sensors.top_left, 0x3000);
        assert_eq!(parsed.sensors.bottom_left, 0x4000);
        assert_eq!(parsed.buttons.raw(), 0);
    }

    #[test]
    fn parses_full_0x34_report_using_first_8_ext_bytes() {
        let mut report = vec![0x34, 0x00, 0x00];
        // First 8 ext bytes are Balance Board; remaining 11 are filler we ignore.
        report.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22]);
        report.extend_from_slice(&[0; 11]);
        let parsed = parse_report(&report).unwrap();
        assert_eq!(parsed.sensors.top_right, 0xAABB);
        assert_eq!(parsed.sensors.bottom_left, 0x1122);
    }

    #[test]
    fn parses_button_a_pressed() {
        // Same 0x32 report with byte 2 = 0x08 (A button).
        let mut report = vec![0x32, 0x00, 0x08];
        report.extend_from_slice(&[0; 8]);
        let parsed = parse_report(&report).unwrap();
        assert!(parsed.buttons.a());
        assert!(parsed.buttons.balance_board_button());
    }

    #[test]
    fn rejects_empty_report() {
        assert_eq!(parse_report(&[]), Err(ParseError::EmptyReport));
    }

    #[test]
    fn rejects_unknown_report_id() {
        let report = [0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            parse_report(&report),
            Err(ParseError::UnsupportedReportId(0xFF))
        );
    }

    #[test]
    fn rejects_truncated_report() {
        let report = [0x32, 0x00, 0x00, 0x10, 0x00]; // missing 6 ext bytes
        assert_eq!(
            parse_report(&report),
            Err(ParseError::TruncatedReport {
                expected: 11,
                got: 5
            })
        );
    }
}
