//! Pure parsing, calibration, and center-of-gravity math for the Nintendo
//! Wii Balance Board (Bluetooth HID device `Nintendo RVL-WBC-01`).
//!
//! This crate does no I/O. Give it bytes off the wire and EEPROM calibration
//! data; it returns typed sensor values, weight in kilograms per sensor, and
//! a normalized center-of-gravity reading.
//!
//! # Example
//!
//! ```
//! use balance_board_protocol::parse_report_extension;
//!
//! // Eight bytes of extension payload extracted from a 0x32 HID report.
//! let ext = [0x10, 0x00, 0x10, 0x00, 0x10, 0x00, 0x10, 0x00];
//! let raw = parse_report_extension(&ext);
//! assert_eq!(raw.top_right, 0x1000);
//! ```
//!
//! # Layered API
//!
//! - [`parse_report`] / [`parse_report_extension`] turn bytes into [`RawSensors`].
//! - [`Calibration::from_eeprom`] parses the 24-byte EEPROM block.
//! - [`Calibration::calibrate`] turns [`RawSensors`] into [`CalibratedSensors`] (kg per corner).
//! - [`CalibratedSensors::center_of_gravity`] gives a normalized [`CenterOfGravity`].

mod buttons;
mod calibration;
mod cog;
mod error;
mod filter;
mod report;
mod sensors;

pub use buttons::WiimoteButtons;
pub use calibration::{CalibratedSensors, Calibration};
pub use cog::CenterOfGravity;
pub use error::{CalibrationError, ParseError};
pub use filter::LowPass2D;
pub use report::{parse_report, parse_report_extension, BoardReport, ReportId};
pub use sensors::{Corner, RawSensors, SensorQuad};
