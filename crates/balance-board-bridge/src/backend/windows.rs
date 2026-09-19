use crate::{
    cache,
    processing::Processed,
    vjoy::{VJoyAxis, VJoyDevice},
    Output, Sample, Source,
};
use balance_board_io::{BalanceBoardSource, HidApiBoard};
use balance_board_protocol::Calibration;
use std::{ffi::CString, io, time::Duration};

struct Board {
    board: HidApiBoard,
    calibration: Calibration,
}

pub(super) fn open(identity: Option<&str>, no_cache: bool) -> io::Result<Box<dyn Source>> {
    let mut board = match identity {
        Some(path) => HidApiBoard::open_path(&CString::new(path).map_err(io::Error::other)?)?,
        None => HidApiBoard::open()?,
    };
    // Stable FNV-1a key isolates calibration by HID path, without exposing
    // Bluetooth addresses in filenames or accepting the old shared cache.
    let hash = board
        .path()
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
    let dir = cache::user_cache_dir().map(|dir| dir.join(format!("{hash:016x}")));
    let cached = if no_cache {
        None
    } else {
        dir.as_deref()
            .and_then(cache::load_from)
            .and_then(|bytes| Calibration::from_eeprom(&bytes).ok())
    };
    let calibration = match cached {
        Some(calibration) => calibration,
        None => {
            let bytes = board.read_calibration_block()?;
            let calibration = Calibration::from_eeprom(&bytes).map_err(io::Error::other)?;
            if let Some(dir) = dir {
                let _ = cache::save_to(&dir, &bytes);
            }
            calibration
        }
    };
    Ok(Box::new(Board { board, calibration }))
}

impl Source for Board {
    fn identity(&self) -> String {
        self.board.path().to_string_lossy().into_owned()
    }
    fn poll(&mut self) -> io::Result<Option<Sample>> {
        match self.board.next_report_timeout(Duration::from_millis(250)) {
            Ok(report) => Ok(Some(Sample {
                weights: self.calibration.calibrate(report.sensors),
                button: report.buttons.balance_board_button(),
            })),
            Err(error) if error.kind() == io::ErrorKind::TimedOut => Ok(None),
            Err(error) => Err(error),
        }
    }
}

pub(super) fn output() -> io::Result<Box<dyn Output>> {
    Ok(Box::new(VJoyDevice::acquire(1)?))
}

impl Output for VJoyDevice {
    fn send(&mut self, p: &Processed) -> io::Result<()> {
        self.set_axis_normalized(VJoyAxis::X, p.cog_x)?;
        self.set_axis_normalized(VJoyAxis::Y, p.cog_y)?;
        for (axis, value) in [VJoyAxis::Z, VJoyAxis::Rx, VJoyAxis::Ry, VJoyAxis::Rz]
            .into_iter()
            .zip(p.corner_axes)
        {
            self.set_axis_normalized(axis, value)?;
        }
        self.set_button(1, p.button)
    }
    fn neutralize(&mut self) -> io::Result<()> {
        VJoyDevice::neutralize(self)
    }
}
