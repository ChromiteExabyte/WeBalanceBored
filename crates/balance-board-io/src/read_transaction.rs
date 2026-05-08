//! Reassembling Wiimote register-read responses (report ID `0x21`).
//!
//! A single `0x17` read-register command can ask for up to 65 535 bytes; the
//! board fragments the response into 16-byte chunks, each carrying its own
//! source address. This module pieces the chunks back together, tolerant of
//! ordering and of sensor reports interleaving on the same HID stream.

use std::io;

/// State machine that assembles `0x21` register-read response frames into
/// the originally-requested byte range.
pub struct ReadTransaction {
    base_addr: u32,
    expected_len: u16,
    buffer: Vec<u8>,
    bytes_received: usize,
}

impl ReadTransaction {
    /// Create a transaction expecting `expected_len` bytes starting at
    /// `base_addr` (in extension-register address space).
    pub fn new(base_addr: u32, expected_len: u16) -> Self {
        Self {
            base_addr,
            expected_len,
            buffer: vec![0u8; expected_len as usize],
            bytes_received: 0,
        }
    }

    /// `true` once every requested byte has been received.
    pub fn is_complete(&self) -> bool {
        self.bytes_received >= self.expected_len as usize
    }

    /// Consume the assembled bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }

    /// Process one `0x21` response frame.
    ///
    /// Frame layout (22 bytes total on the wire):
    ///
    /// | Offset | Length | Meaning                                      |
    /// |--------|--------|----------------------------------------------|
    /// | 0      | 1      | Report ID (`0x21`)                           |
    /// | 1..=2  | 2      | Core button state (ignored here)             |
    /// | 3      | 1      | High nibble = error, low nibble = size − 1   |
    /// | 4..=5  | 2      | Source address, low 16 bits, big-endian      |
    /// | 6..=21 | 16     | Payload (`size` valid bytes, rest zero pad)  |
    pub fn consume(&mut self, frame: &[u8]) -> io::Result<()> {
        if frame.len() < 22 || frame[0] != 0x21 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a 0x21 register-read response frame",
            ));
        }
        let err_size = frame[3];
        let err = err_size >> 4;
        let size = (err_size & 0x0F) as usize + 1;
        if err != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Wiimote register-read error code 0x{err:x}"),
            ));
        }
        let frame_addr_low = u16::from_be_bytes([frame[4], frame[5]]) as u32;
        let base_low = self.base_addr & 0xFFFF;
        let offset = frame_addr_low.wrapping_sub(base_low) as usize;

        if offset + size > self.expected_len as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "register-read response exceeds requested length",
            ));
        }
        self.buffer[offset..offset + size].copy_from_slice(&frame[6..6 + size]);
        self.bytes_received += size;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(addr_low: u16, payload: &[u8]) -> Vec<u8> {
        assert!(!payload.is_empty() && payload.len() <= 16);
        let mut f = vec![0u8; 22];
        f[0] = 0x21;
        // err=0, size = payload.len() - 1
        f[3] = ((payload.len() - 1) & 0x0F) as u8;
        f[4..6].copy_from_slice(&addr_low.to_be_bytes());
        f[6..6 + payload.len()].copy_from_slice(payload);
        f
    }

    #[test]
    fn assembles_two_frames_in_order() {
        let mut tx = ReadTransaction::new(0x00A4_0024, 24);
        // Frame 1: 16 bytes at low addr 0x0024.
        tx.consume(&frame(0x0024, &[0x01; 16])).unwrap();
        assert!(!tx.is_complete());
        // Frame 2: 8 bytes at low addr 0x0034.
        tx.consume(&frame(0x0034, &[0x02; 8])).unwrap();
        assert!(tx.is_complete());

        let bytes = tx.into_bytes();
        assert_eq!(bytes.len(), 24);
        assert_eq!(&bytes[..16], &[0x01; 16]);
        assert_eq!(&bytes[16..], &[0x02; 8]);
    }

    #[test]
    fn assembles_two_frames_out_of_order() {
        let mut tx = ReadTransaction::new(0x00A4_0024, 24);
        tx.consume(&frame(0x0034, &[0x02; 8])).unwrap();
        tx.consume(&frame(0x0024, &[0x01; 16])).unwrap();
        assert!(tx.is_complete());
        let bytes = tx.into_bytes();
        assert_eq!(&bytes[..16], &[0x01; 16]);
        assert_eq!(&bytes[16..], &[0x02; 8]);
    }

    #[test]
    fn rejects_non_read_response() {
        let mut tx = ReadTransaction::new(0, 8);
        let mut bad = vec![0u8; 22];
        bad[0] = 0x32; // wrong report id
        assert!(tx.consume(&bad).is_err());
    }

    #[test]
    fn rejects_short_frame() {
        let mut tx = ReadTransaction::new(0, 8);
        let short = vec![0x21u8; 10];
        assert!(tx.consume(&short).is_err());
    }

    #[test]
    fn surfaces_wiimote_error_code() {
        let mut tx = ReadTransaction::new(0, 8);
        let mut bad = vec![0u8; 22];
        bad[0] = 0x21;
        bad[3] = 0x70; // err=7, size=1
        let err = tx.consume(&bad).unwrap_err();
        assert!(err.to_string().contains("0x7"));
    }

    #[test]
    fn rejects_overflow() {
        let mut tx = ReadTransaction::new(0x00A4_0024, 8);
        // Frame claims 16 bytes at offset 0 — exceeds expected 8.
        assert!(tx.consume(&frame(0x0024, &[0xAB; 16])).is_err());
    }
}
