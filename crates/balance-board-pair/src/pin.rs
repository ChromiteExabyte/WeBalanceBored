//! Wii binary PIN encoding. Red SYNC pairing uses the local Bluetooth radio's
//! address, in Win32 rgBytes order. Wii Remote 1+2 pairing uses a different
//! address convention and is not the Balance Board pairing flow here.
//!
//! References:
//! <https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/HW/WiimoteReal/IOWin.cpp>
//! <https://github.com/bluez/bluez/blob/master/plugins/autopair.c>

/// Length of a Wii pairing PIN, in bytes. Equal to the Bluetooth
/// address length.
pub const WII_PIN_LEN: usize = 6;

/// Preserve a Bluetooth address supplied in Win32 rgBytes order as PIN bytes.
/// The caller must select the address appropriate to the pairing mode:
/// the local radio address for red SYNC, not the remote board address.
#[must_use]
pub fn wii_pin_for_address(rg_bytes: [u8; 6]) -> [u8; WII_PIN_LEN] {
    rg_bytes
}

/// Encode the local radio address for Windows' direct legacy authentication.
/// Each byte occupies its own WCHAR; an extra NUL terminates the buffer.
/// Always pass WII_PIN_LEN (6) as the explicit length, including embedded zeros.
#[must_use]
pub fn sync_passkey(local_radio_address: [u8; WII_PIN_LEN]) -> [u16; WII_PIN_LEN + 1] {
    let mut wide = [0; WII_PIN_LEN + 1];
    for (slot, byte) in wide
        .iter_mut()
        .zip(wii_pin_for_address(local_radio_address))
    {
        *slot = u16::from(byte);
    }
    wide
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
        // Address-byte conversion only; pairing chooses the local radio.
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
    fn legacy_passkey_preserves_zero_and_high_bytes_without_packing() {
        assert_eq!(
            sync_passkey([0x04, 0xb1, 0x9e, 0xef, 0xdc, 0xb0]),
            [0x04, 0xb1, 0x9e, 0xef, 0xdc, 0xb0, 0]
        );
        assert_eq!(
            sync_passkey([0, 1, 0x80, 0xff, 0, 6]),
            [0, 1, 0x80, 0xff, 0, 6, 0]
        );
    }

    #[test]
    fn pin_length_is_six() {
        assert_eq!(WII_PIN_LEN, 6);
        let pin = wii_pin_for_address([0; 6]);
        assert_eq!(pin.len(), 6);
    }
}
