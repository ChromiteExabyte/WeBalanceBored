//! Wiimote-style button bitfield from core report bytes 1–2.
//!
//! Every 0x32 / 0x34 HID report from a Wiimote-family device starts
//! with `[report_id, core_buttons_lo, core_buttons_hi, ...]`. The two
//! button bytes encode the standard Wiimote control set:
//!
//! ```text
//! Byte 1 (low):                Byte 2 (high):
//!   bit 0  D-pad Left            bit 0  Two
//!   bit 1  D-pad Right           bit 1  One
//!   bit 2  D-pad Down            bit 2  B
//!   bit 3  D-pad Up              bit 3  A
//!   bit 4  Plus                  bit 4  Minus
//!                                bit 7  Home
//! ```
//!
//! The Balance Board has a single physical button on its front edge
//! (the power / SYNC button). When that button is pressed during an
//! active connection it shows up in this same field — typically as
//! the "A" bit. We expose the full bitfield rather than guessing,
//! and provide a [`balance_board_button`](WiimoteButtons::balance_board_button)
//! convenience that returns the OR of every bit so callers don't have
//! to care about the exact firmware mapping.

/// Decoded Wiimote button state from a single core-buttons report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WiimoteButtons {
    raw: u16,
}

impl WiimoteButtons {
    /// Construct from the two raw bytes at offsets 1 and 2 of a
    /// Wiimote report. Layout per WiiBrew (low byte first).
    #[must_use]
    pub const fn from_bytes(b1: u8, b2: u8) -> Self {
        Self {
            raw: u16::from_le_bytes([b1, b2]),
        }
    }

    /// The packed bitfield. Useful when you need the full state in a
    /// single place — e.g. comparing against a previous frame to
    /// detect rising or falling edges.
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.raw
    }

    /// `true` if any button bit is set.
    #[must_use]
    pub const fn any_pressed(self) -> bool {
        self.raw != 0
    }

    /// The Balance Board's single physical button. Returns `true` if
    /// any of the standard Wiimote button bits is set, since
    /// firmwares vary on which exact bit the Balance Board's
    /// power/SYNC button toggles. Effectively: "the user pressed
    /// the button on the front of the board."
    #[must_use]
    pub const fn balance_board_button(self) -> bool {
        self.any_pressed()
    }

    // Individual Wiimote buttons. Useful if you want to drive a real
    // Wiimote with this crate, or if you need to disambiguate the
    // Balance Board's bit if your firmware is unusual.

    /// Wiimote D-pad Left (`0x0001`).
    #[must_use] pub const fn dpad_left(self)  -> bool { self.raw & 0x0001 != 0 }
    /// Wiimote D-pad Right (`0x0002`).
    #[must_use] pub const fn dpad_right(self) -> bool { self.raw & 0x0002 != 0 }
    /// Wiimote D-pad Down (`0x0004`).
    #[must_use] pub const fn dpad_down(self)  -> bool { self.raw & 0x0004 != 0 }
    /// Wiimote D-pad Up (`0x0008`).
    #[must_use] pub const fn dpad_up(self)    -> bool { self.raw & 0x0008 != 0 }
    /// Wiimote Plus (`0x0010`).
    #[must_use] pub const fn plus(self)       -> bool { self.raw & 0x0010 != 0 }
    /// Wiimote 2 (`0x0100`).
    #[must_use] pub const fn two(self)        -> bool { self.raw & 0x0100 != 0 }
    /// Wiimote 1 (`0x0200`).
    #[must_use] pub const fn one(self)        -> bool { self.raw & 0x0200 != 0 }
    /// Wiimote B (`0x0400`).
    #[must_use] pub const fn b(self)          -> bool { self.raw & 0x0400 != 0 }
    /// Wiimote A (`0x0800`).
    #[must_use] pub const fn a(self)          -> bool { self.raw & 0x0800 != 0 }
    /// Wiimote Minus (`0x1000`).
    #[must_use] pub const fn minus(self)      -> bool { self.raw & 0x1000 != 0 }
    /// Wiimote Home (`0x8000`).
    #[must_use] pub const fn home(self)       -> bool { self.raw & 0x8000 != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_pressed() {
        let b = WiimoteButtons::from_bytes(0, 0);
        assert!(!b.any_pressed());
        assert!(!b.balance_board_button());
        assert_eq!(b.raw(), 0);
    }

    #[test]
    fn a_button_bit() {
        // A is byte 2 bit 3 = 0x08, packed le → 0x0800.
        let b = WiimoteButtons::from_bytes(0x00, 0x08);
        assert!(b.a());
        assert!(b.any_pressed());
        assert!(b.balance_board_button());
        assert_eq!(b.raw(), 0x0800);
    }

    #[test]
    fn dpad_bits_in_low_byte() {
        let b = WiimoteButtons::from_bytes(0x0F, 0x00);
        assert!(b.dpad_left() && b.dpad_right() && b.dpad_down() && b.dpad_up());
        assert!(!b.plus() && !b.a());
    }

    #[test]
    fn plus_minus_home() {
        let b = WiimoteButtons::from_bytes(0x10, 0x80 | 0x10);
        assert!(b.plus());
        assert!(b.minus());
        assert!(b.home());
    }

    #[test]
    fn balance_board_button_is_any_press() {
        // Even if a non-A bit is set (some firmwares route the SYNC
        // button differently), balance_board_button should return true.
        for byte in 0x01..=0xFF {
            let b = WiimoteButtons::from_bytes(byte, 0);
            assert!(b.any_pressed());
            assert!(b.balance_board_button(), "low byte = {byte:#x}");
        }
    }

    #[test]
    fn from_bytes_is_const() {
        // Compile-time evaluation check.
        const B: WiimoteButtons = WiimoteButtons::from_bytes(0x10, 0x08);
        assert_eq!(B.raw(), 0x0810);
        assert!(B.plus());
        assert!(B.a());
    }
}
