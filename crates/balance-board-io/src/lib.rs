//! HID I/O for the Wii Balance Board.
//!
//! Wraps the cross-platform [`hidapi`] crate to discover and communicate
//! with a paired Balance Board (`Nintendo RVL-WBC-01`), exposing the live
//! sensor stream and EEPROM calibration data via [`BalanceBoardSource`].
//!
//! Bluetooth pairing is out of scope — pair the board through your OS's
//! Bluetooth UI before running. The board's PIN is its own Bluetooth MAC
//! address with the bytes reversed; see WiiBrew for details.
//!
//! # Example
//!
//! ```no_run
//! use balance_board_io::{BalanceBoardSource, HidApiBoard};
//! use balance_board_protocol::Calibration;
//!
//! let mut board = HidApiBoard::open()?;
//! let cal = Calibration::from_eeprom(&board.read_calibration_block()?)?;
//! loop {
//!     let raw = board.next_report()?;
//!     let kg = cal.calibrate(raw);
//!     println!("{:.1} kg", kg.total_kg());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(missing_docs)]

mod hidapi_source;
mod read_transaction;

pub use hidapi_source::HidApiBoard;

use balance_board_protocol::RawSensors;
use std::io;

/// A live source of raw Balance Board sensor reports.
///
/// Implementations wrap whatever HID transport is in use. The default
/// implementation is [`HidApiBoard`], which works on Windows, macOS, and
/// Linux via the hidapi C library.
pub trait BalanceBoardSource {
    /// Block until the next sensor report arrives, then return it.
    ///
    /// Non-sensor reports (status, register-read responses) that interleave
    /// with the sensor stream are silently skipped.
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the underlying transport fails (device
    /// unplugged, Bluetooth dropped, OS-level read error, etc.).
    fn next_report(&mut self) -> io::Result<RawSensors>;

    /// Read the 24-byte EEPROM calibration block from the board.
    ///
    /// Issues a Wiimote extension-register read at `0xa40024..=0xa4003b`
    /// and reassembles the multi-frame response. Call once per session,
    /// before the first sensor report is consumed (the calibration read
    /// races with the sensor stream and is easier to reason about up front).
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the read transaction fails or times out.
    fn read_calibration_block(&mut self) -> io::Result<[u8; 24]>;
}
