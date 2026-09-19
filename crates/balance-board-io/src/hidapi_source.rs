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
/// we prefer the product string `RVL-WBC-01` when available.
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
    path: CString,
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
    /// - [`io::ErrorKind::NotFound`] if hidapi exposes no matching VID/PID candidate.
    /// - [`io::ErrorKind::Other`] for any underlying hidapi error.
    ///
    /// # Discovery heuristic
    ///
    /// Prefer the product string `Nintendo RVL-WBC-01`. If the HID product
    /// string is missing or generic, fall back to matching by VID + PID
    /// alone (Nintendo `0x057E`, PID `0x0306`, also used by Wiimotes).
    /// Windows' PnP friendly name does not establish what hidapi returns;
    /// use the `list_hid_devices` example to inspect that directly.
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
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "No Balance Board candidate found via hidapi (VID=0x{NINTENDO_VID:04X}, \
                         PID (hex) in {BALANCE_BOARD_PIDS:04x?}).\n\n\
                         A Windows pairing record does not guarantee a usable HID interface. \
                         Missing or generic product strings are already accepted.\n\
                         Wake the board with its front Power button, then inspect what hidapi sees:\n  \
                         cargo run --release --locked -p balance-board-io --example list_hid_devices\n\n\
                         See docs/troubleshooting.md for connection checks and bug-report details."
                    ),
                )
            })?;

        if candidates.len() > 1 {
            eprintln!(
                "warning: {} devices share the Wii Remote/Balance Board VID+PID; selected {:?} \
                 (product: {:?}). Run `list_hid_devices` to inspect candidates; \
                 disconnect other Wii devices if the wrong one is selected.",
                candidates.len(),
                chosen.path(),
                chosen.product_string(),
            );
        }

        let path = chosen.path().to_owned();
        let device = api.open_path(&path).map_err(io_err)?;
        device.set_blocking_mode(true).map_err(io_err)?;

        let mut board = Self {
            _api: api,
            device,
            path,
        };
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
        let mut board = Self {
            _api: api,
            device,
            path: path.to_owned(),
        };
        board.disable_extension_encryption()?;
        board.set_reporting_mode(REPORTING_MODE_BB)?;
        Ok(board)
    }

    /// The selected HID path, for reconnecting to the same device.
    pub fn path(&self) -> &CString {
        &self.path
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
        read_sensor_report(
            |buf, timeout_ms| self.device.read_timeout(buf, timeout_ms).map_err(io_err),
            Duration::from_secs(3),
        )
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

fn read_sensor_report(
    mut read: impl FnMut(&mut [u8], i32) -> io::Result<usize>,
    timeout: Duration,
) -> io::Result<BoardReport> {
    let deadline = std::time::Instant::now() + timeout;
    let mut buf = [0u8; 32];
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut,
                    "No valid sensor report before the deadline. The board may be asleep or disconnected."));
        }
        let timeout_ms = remaining.as_millis().clamp(1, 250) as i32;
        let n = read(&mut buf, timeout_ms)?;
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

fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_status_and_malformed_reports_before_sensor_data() {
        let mut frames = vec![
            vec![0x20, 0, 0],
            vec![0x32],
            vec![0x32, 0, 0, 0, 1, 0, 2, 0, 3, 0, 4],
        ]
        .into_iter();
        let report = read_sensor_report(
            |buf, _| {
                let frame = frames.next().expect("should stop at the sensor report");
                buf[..frame.len()].copy_from_slice(&frame);
                Ok(frame.len())
            },
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(report.sensors.top_right, 1);
        assert_eq!(report.sensors.bottom_left, 4);
    }

    #[test]
    fn unrelated_reports_do_not_extend_deadline() {
        let error = read_sensor_report(
            |buf, _| {
                buf[0] = 0x20;
                Ok(1)
            },
            Duration::from_millis(10),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn empty_reads_time_out() {
        let error = read_sensor_report(|_, _| Ok(0), Duration::from_millis(10)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn transport_failure_is_preserved() {
        let error = read_sensor_report(
            |_, _| Err(io::ErrorKind::BrokenPipe.into()),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    }
}
