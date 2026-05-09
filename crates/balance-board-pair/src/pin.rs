//! Wii Balance Board pairing-PIN derivation.
//!
//! Wii devices use an unusual pairing scheme. Instead of a numeric PIN
//! the user types, the PIN *is* the device's own Bluetooth address —
//! 6 raw bytes — used as a binary passkey.
//!
//! Crucially, the Win32 `BLUETOOTH_ADDRESS.rgBytes` field already
//! stores the address in the byte order WiiBrew describes as "reversed
//! BD_ADDR." On a little-endian Windows host, `rgBytes[0]` is the
//! least-significant byte — which is the "first" byte of the PIN as
//! the board expects it. So the derivation is just: pass the rgBytes
//! array through unchanged.
//!
//! This module exists primarily to:
//! 1. Encode the convention as a typed function (`wii_pin_for_address`).
//! 2. Provide a readable formatter for diagnostic output.
//! 3. Hold the unit tests that pin (heh) the convention so future
//!    refactors don't accidentally swap byte order.

/// Length of a Wii pairing PIN, in bytes. Equal to the Bluetooth
/// address length.
pub const WII_PIN_LEN: usize = 6;

/// Compute the binary PIN to send when pairing with a Wii device.
///
/// Per WiiBrew + the original WiiBalanceWalker + hardware testing:
/// the PIN is the **device's own** Bluetooth address (Wiimote or
/// Balance Board), in Win32 rgBytes order (little-endian, equivalent
/// to "BD_ADDR reversed" if you read addresses big-endian like
/// most Bluetooth UIs do).
///
/// (Older WiiBrew text suggests the host MAC for SYNC pairing.
/// On Carter's Windows machine that caused
/// `BluetoothSendAuthenticationResponseEx` to hang waiting for a
/// device acknowledgment that never came; switching to the device's
/// own MAC matches what the original 32feet.NET-based WiiBalanceWalker
/// did and what other Wii pairing tools use.)
///
/// Input: a Bluetooth address as exposed by Win32
/// (`BLUETOOTH_ADDRESS.Anonymous.rgBytes`), already in little-endian
/// byte order.
///
/// Output: 6 bytes ready to feed straight into
/// `BLUETOOTH_AUTHENTICATE_RESPONSE.pinInfo.pin[..6]`.
#[must_use]
pub fn wii_pin_for_address(rg_bytes: [u8; 6]) -> [u8; WII_PIN_LEN] {
    rg_bytes
}

/// Format a PIN as colon-separated uppercase hex (e.g.
/// `00:26:59:31:2F:A7`). Useful for diagnostics — the binary PIN
/// itself is opaque.
#[must_use]
pub fn format_pin(pin: [u8; WII_PIN_LEN]) -> String {
    let mut s = String::with_capacity(WII_PIN_LEN * 3 - 1);
    for (i, b) in pin.iter().enumerate() {
        if i > 0 {
            s.push(':');
        }
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// Format a Bluetooth address (Win32 rgBytes order) as colon-separated
/// uppercase hex in the human-readable big-endian convention used by
/// most Bluetooth UIs (e.g. `A7:2F:31:59:26:00` from rgBytes
/// `[0x00, 0x26, 0x59, 0x31, 0x2F, 0xA7]`).
#[must_use]
pub fn format_bd_addr(rg_bytes: [u8; 6]) -> String {
    let mut s = String::with_capacity(6 * 3 - 1);
    for (i, b) in rg_bytes.iter().rev().enumerate() {
        if i > 0 {
            s.push(':');
        }
        s.push_str(&format!("{b:02X}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_is_rg_bytes_unchanged() {
        // Carter's board (from the Get-PnpDevice output in the bug
        // report): BTHENUM\DEV_002659312FA7
        // Big-endian display: 00:26:59:31:2F:A7
        // Win32 rgBytes (little-endian): A7, 2F, 31, 59, 26, 00
        let rg = [0xA7, 0x2F, 0x31, 0x59, 0x26, 0x00];
        let pin = wii_pin_for_address(rg);
        assert_eq!(pin, rg, "PIN must be rgBytes verbatim");
    }

    #[test]
    fn pin_format_is_colon_hex_in_rg_byte_order() {
        let pin = [0xA7, 0x2F, 0x31, 0x59, 0x26, 0x00];
        assert_eq!(format_pin(pin), "A7:2F:31:59:26:00");
    }

    #[test]
    fn bd_addr_formats_in_human_readable_big_endian() {
        // Same address as above; UI should show the conventional form.
        let rg = [0xA7, 0x2F, 0x31, 0x59, 0x26, 0x00];
        assert_eq!(format_bd_addr(rg), "00:26:59:31:2F:A7");
    }

    #[test]
    fn pin_length_is_six() {
        assert_eq!(WII_PIN_LEN, 6);
        let pin = wii_pin_for_address([0; 6]);
        assert_eq!(pin.len(), 6);
    }
}
