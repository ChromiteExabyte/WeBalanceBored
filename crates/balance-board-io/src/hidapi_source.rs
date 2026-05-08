//! `hidapi`-backed Balance Board source.
//!
//! Implements the Wiimote handshake (disable extension encryption, switch
//! to a balance-board-friendly reporting mode) and exposes the live sensor
//! stream plus EEPROM calibration via [`BalanceBoardSource`].

use balance_board_protocol::{parse_report, BoardReport};
use hidapi::{HidApi, HidDevice};
use std::ffi::CString;
use std::io;
use std::time::Duration;

use crate::read_transaction::ReadTransaction;
use crate::BalanceBoardSource;

/// USB/HID vendor ID for Nintendo.
const NINTENDO_VID: u16 = 0x057E;

/// Product IDs that have shown up for the Balance Board across firmwares.
/// The board identifies itself with the same PID as a standard Wiimote;
/// we disambiguate by the product string `RVL-WBC-01`.
const BALANCE_BOARD_PIDS: &[u16] = &[0x0306];

/// Address of the 24-byte calibration block in extension-register space.
const CAL_BLOCK_ADDR: u32 = 0x00A4_0024;
/// Length of the calibration block.
const CAL_BLOCK_LEN: u16 = 24;

/// Output report 0x12: set reporting mode.
const RPT_SET_MODE: u8 = 0x12;
/// Output report 0x16: write to control registers.
const RPT_WRITE_REG: u8 = 0x16;
/// Output report 0x17: read from control registers.
const RPT_READ_REG: u8 = 0x17;
/// Address space byte for extension/control registers.
const ADDR_SPACE_REGISTERS: u8 = 0x04;
/// Reporting-mode flags: continuous reports (bit 2 set), no rumble.
const REPORT_FLAGS_CONTINUOUS: u8 = 0x04;
/// Reporting mode: Core Buttons + 8 Extension. The smallest report that
/// carries the full Balance Board payload.
const REPORTING_MODE_BB: u8 = 0x32;

/// A Balance Board accessed through `hidapi`.
pub struct HidApiBoard {
    device: HidDevice,
    /// Kept alive for the lifetime of the device. hidapi-rs's global state
    /// is reference-counted; holding the context defensively avoids any
    /// teardown surprises if the user opens multiple boards.
    _api: HidApi,
}

impl HidApiBoard {
    /// Discover and open the first paired Balance Board on the system.
    ///
    /// Performs the Wiimote-extension handshake (disable encryption, switch
    /// to reporting mode 0x32) before returning, so the device is ready to
    /// stream sensor data.
    ///
    /// # Errors
    /// - [`io::ErrorKind::NotFound`] if no Balance Board is paired.
    /// - [`io::ErrorKind::Other`] for any underlying hidapi error.
    ///
    /// # Discovery heuristic
    ///
    /// On most platforms a Balance Board reports product string
    /// `Nintendo RVL-WBC-01`, so we prefer that. But on Windows after a
    /// Bluetooth pairing, hidapi often only exposes the HID *child*
    /// object whose product string is generic (e.g. `HID-compliant game
    /// controller`); the friendly Bluetooth-level name is only on the
    /// parent. To handle that, we fall back to matching by VID + PID
    /// alone (Nintendo `0x057E`, PID `0x0306` — same as a Wiimote).
    ///
    /// Edge case: if you have a Wiimote and a Balance Board paired at
    /// the same time and neither exposes a distinguishing product
    /// string, the first match wins, which may be wrong. In that case
    /// run the `list_hid_devices` example to see all candidates and
    /// open the right one explicitly via [`open_path`](Self::open_path).
    pub fn open() -> io::Result<Self> {
        let api = HidApi::new().map_err(io_err)?;

        let candidates: Vec<&hidapi::DeviceInfo> = api
            .device_list()
            .filter(|info| {
                info.vendor_id() == NINTENDO_VID && BALANCE_BOARD_PIDS.contains(&info.product_id())
            })
            .collect();

        let chosen = candidates
            .iter()
            .copied()
            .find(|info| {
                info.product_string()
                    .map(|s| s.contains("RVL-WBC-01"))
                    .unwrap_or(false)
            })
            .or_else(|| candidates.first().copied())
            .ok_or_else(|| {
                io::Error::other(format!(
                    "No Balance Board found via hidapi (looking for VID=0x{NINTENDO_VID:04X}, \
                     PID one of {BALANCE_BOARD_PIDS:#06x?}).\n\n\
                     If Windows shows `Nintendo RVL-WBC-01` under Bluetooth Settings but this \
                     binary still can't see it, the board's HID child may have a generic \
                     product string. Run the `list_hid_devices` example to see what hidapi \
                     reports on this machine:\n  \
                     cargo run -p balance-board-io --example list_hid_devices\n\n\
                     If the board isn't paired yet, pair `Nintendo RVL-WBC-01` via Windows \
                     Bluetooth Settings first."
                ))
            })?;

        if candidates.len() > 1 {
            eprintln!(
                "warning: {} devices match Nintendo VID+PID; picking first ({:?}). \
                 Use `list_hid_devices` + open_path() if this is wrong.",
                candidates.len(),
                chosen.path(),
            );
        }

        let path = chosen.path().to_owned();
        let device = api.open_path(&path).map_err(io_err)?;
        device.set_blocking_mode(true).map_err(io_err)?;

        let mut board = Self { _api: api, device };
        board.disable_extension_encryption()?;
        board.set_reporting_mode(REPORTING_MODE_BB)?;
        Ok(board)
    }

    /// Open a Balance Board by an explicit HID device path.
    ///
    /// Useful when you want to bypass auto-discovery (e.g. multiple boards,
    /// or a non-standard PID). The handshake is still performed.
    pub fn open_path(path: &CString) -> io::Result<Self> {
        let api = HidApi::new().map_err(io_err)?;
        let device = api.open_path(path).map_err(io_err)?;
        device.set_blocking_mode(true).map_err(io_err)?;
        let mut board = Self { _api: api, device };
        board.disable_extension_encryption()?;
        board.set_reporting_mode(REPORTING_MODE_BB)?;
        Ok(board)
    }

    fn disable_extension_encryption(&mut self) -> io::Result<()> {
        // The two-write "new init" sequence — works on every Wiimote and
        // Balance Board firmware including the post-TR generation.
        //
        // - Write 0x55 to register 0xA400F0  → disable extension encryption.
        // - Write 0x00 to register 0xA400FB  → finish the init handshake.
        //
        // The second write returns an error on some firmwares; ignoring it
        // is safe and matches what jloehr/HID-Wiimote does.
        //
        // References:
        // - WiiBrew Wiimote/Extension_Controllers § "The New Way":
        //   https://wiibrew.org/wiki/Wiimote/Extension_Controllers#The_New_Way
        // - jloehr/HID-Wiimote Wiimote.c — same byte sequence.
        self.write_register(0x00A4_00F0, &[0x55])?;
        let _ = self.write_register(0x00A4_00FB, &[0x00]);
        Ok(())
    }

    fn set_reporting_mode(&mut self, mode_id: u8) -> io::Result<()> {
        let buf = [RPT_SET_MODE, REPORT_FLAGS_CONTINUOUS, mode_id];
        self.device.write(&buf).map_err(io_err)?;
        Ok(())
    }

    fn write_register(&mut self, addr: u32, data: &[u8]) -> io::Result<()> {
        if data.is_empty() || data.len() > 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "register write payload must be 1..=16 bytes",
            ));
        }
        let mut buf = [0u8; 22];
        buf[0] = RPT_WRITE_REG;
        buf[1] = ADDR_SPACE_REGISTERS;
        buf[2] = ((addr >> 16) & 0xFF) as u8;
        buf[3] = ((addr >> 8) & 0xFF) as u8;
        buf[4] = (addr & 0xFF) as u8;
        buf[5] = data.len() as u8;
        buf[6..6 + data.len()].copy_from_slice(data);
        self.device.write(&buf).map_err(io_err)?;
        Ok(())
    }

    fn read_register(&mut self, addr: u32, len: u16) -> io::Result<Vec<u8>> {
        let mut buf = [0u8; 7];
        buf[0] = RPT_READ_REG;
        buf[1] = ADDR_SPACE_REGISTERS;
        buf[2] = ((addr >> 16) & 0xFF) as u8;
        buf[3] = ((addr >> 8) & 0xFF) as u8;
        buf[4] = (addr & 0xFF) as u8;
        buf[5] = ((len >> 8) & 0xFF) as u8;
        buf[6] = (len & 0xFF) as u8;
        self.device.write(&buf).map_err(io_err)?;

        let timeout = Duration::from_millis(2000);
        let deadline = std::time::Instant::now() + timeout;
        let mut tx = ReadTransaction::new(addr, len);
        let mut report = [0u8; 32];
        while !tx.is_complete() {
            if std::time::Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "no register-read response within 2s",
                ));
            }
            let n = self.device.read_timeout(&mut report, 250).map_err(io_err)?;
            if n == 0 {
                continue;
            }
            // 0x21 is the register-read response; everything else (sensor
            // reports streaming in parallel) we ignore here.
            if report[0] == 0x21 {
                tx.consume(&report[..n])?;
            }
        }
        Ok(tx.into_bytes())
    }
}

impl BalanceBoardSource for HidApiBoard {
    fn next_report(&mut self) -> io::Result<BoardReport> {
        let mut buf = [0u8; 32];
        loop {
            let n = self.device.read(&mut buf).map_err(io_err)?;
            if n == 0 {
                continue;
            }
            // Skip anything that isn't a recognized sensor report —
            // status reports (0x20) and read responses (0x21) interleave
            // with the data stream and don't carry sensor values.
            if let Ok(report) = parse_report(&buf[..n]) {
                return Ok(report);
            }
        }
    }

    fn read_calibration_block(&mut self) -> io::Result<[u8; 24]> {
        let bytes = self.read_register(CAL_BLOCK_ADDR, CAL_BLOCK_LEN)?;
        bytes.try_into().map_err(|v: Vec<u8>| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected 24 bytes of calibration, got {}", v.len()),
            )
        })
    }
}

fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}
