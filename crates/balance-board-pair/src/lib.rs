//! Auto-pair the Wii Balance Board on Windows.
//!
//! Wii devices can't be paired through the standard Windows Bluetooth
//! wizard without help — they expect a binary 6-byte PIN equal to
//! the local radio's Bluetooth address for red SYNC pairing (see [`pin`]), instead of the
//! 4–6 digit decimal passkey the wizard prompts for. This crate
//! automates the same handshake the wizard would do, but with the
//! correct Wii PIN, so the user just runs:
//!
//! ```pwsh
//! balance-board-pair          # press SYNC, this finds + pairs the board
//! balance-board-pair --scan   # list nearby Wii devices, no pairing
//! balance-board-pair --forget # unpair every Balance Board
//! ```
//!
//! After pairing succeeds, the Balance Board appears as a normal HID
//! device that [`balance_board_io::HidApiBoard`] (in another crate)
//! can find via hidapi.
//!
//! # Why Windows-only
//!
//! Pairing requires platform-specific Bluetooth APIs. The Win32
//! surface is one we can drive directly via `windows-sys`. Linux
//! uses the launcher's BlueZ flow instead. Pairing success still needs
//! hardware verification on each adapter and operating system.
//!
//! [`balance_board_io::HidApiBoard`]: https://docs.rs/balance-board-io

#![warn(missing_docs)]

pub mod pin;

#[cfg(windows)]
mod bluetooth;

#[cfg(windows)]
pub use bluetooth::{forget_all_balance_boards, pair_first, scan, WiiDevice};
