//! Error types. Hand-rolled to keep the crate dep-free.

use core::fmt;

/// Errors returned by [`crate::parse_report`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The report buffer was empty.
    EmptyReport,
    /// Report ID byte does not correspond to a supported HID report.
    UnsupportedReportId(u8),
    /// Report ID was recognized but the buffer is shorter than the report demands.
    TruncatedReport {
        /// Minimum bytes required for this report ID.
        expected: usize,
        /// Bytes actually provided.
        got: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::EmptyReport => f.write_str("HID report buffer was empty"),
            ParseError::UnsupportedReportId(id) => {
                write!(f, "unsupported HID report id: 0x{id:02x}")
            }
            ParseError::TruncatedReport { expected, got } => write!(
                f,
                "HID report truncated: expected at least {expected} bytes, got {got}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Errors returned by [`crate::Calibration::from_eeprom`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    /// Calibration values are not strictly increasing across the 0/17/34 kg
    /// reference points. Almost always means uninitialized EEPROM (all zeros)
    /// or a corrupted read.
    Nonmonotonic,
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalibrationError::Nonmonotonic => f.write_str(
                "calibration constants are not monotonically increasing (need kg0 < kg17 < kg34 per sensor)",
            ),
        }
    }
}

impl std::error::Error for CalibrationError {}
