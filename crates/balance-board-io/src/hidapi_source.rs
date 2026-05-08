//! `hidapi`-backed Balance Board source.
//!
//! Implements the Wiimote handshake (disable extension encryption, switch
//! to a balance-board-friendly reporting mode) and exposes the live sensor
//! stream plus EEPROM calibration via [`BalanceBoardSource`].

use balance_board_protocol::{parse_report, RawSensors};
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
    pub fn open() -> io::Result<Self> {
        let api = HidApi::new().map_err(io_err)?;
        let path = api
            .device_list()
            .find(|info| {
                info.vendor_id() == NINTENDO_VID
                    && BALANCE_BOARD_PIDS.contains(&info.product_id())
                    && info
                        .product_string()
                        .map(|s| s.contains("RVL-WBC-01"))
                        .unwrap_or(false)
            })
            .map(|info| info.path().to_owned())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "No Balance Board found. Pair `Nintendo RVL-WBC-01` via your OS Bluetooth UI first.",
                )
            })?;

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
        // Writing 0x55 to register 0xa400f0 disables encryption on the
        // extension. The follow-up write to 0xa400fb is part of the
        // documented init dance for some firmwares; some boards return
        // an error here and ignoring it is safe.
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
            let n = self
                .device
                .read_timeout(&mut report, 250)
                .map_err(io_err)?;
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
    fn next_report(&mut self) -> io::Result<RawSensors> {
        let mut buf = [0u8; 32];
        loop {
            let n = self.device.read(&mut buf).map_err(io_err)?;
            if n == 0 {
                continue;
            }
            // Skip anything that isn't a recognized sensor report —
            // status reports (0x20) and read responses (0x21) interleave
            // with the data stream and don't carry sensor values.
            if let Ok(raw) = parse_report(&buf[..n]) {
                return Ok(raw);
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
