//! Use hid-wiimote's calibrated evdev interface, without competing with
//! the kernel driver for HID report mode or reading calibration twice.
//! Axis mapping and centikilogram units:
//! https://github.com/torvalds/linux/blob/master/drivers/hid/hid-wiimote-modules.c
use crate::{processing::Processed, Output, Sample, Source};
use balance_board_protocol::CalibratedSensors;
use evdev::{
    uinput::VirtualDevice, AbsInfo, AbsoluteAxisCode as Axis, AbsoluteAxisEvent, AttributeSet,
    Device, InputEvent, KeyCode, KeyEvent, UinputAbsSetup,
};
use std::{fs, io, thread, time::Duration};

struct Board {
    device: Device,
    identity: String,
}

pub(super) fn open(identity: Option<&str>) -> io::Result<Box<dyn Source>> {
    let entries = fs::read_dir("/sys/class/input")?;
    let mut permission_denied = false;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("event") {
            continue;
        }
        let device_name = fs::read_to_string(entry.path().join("device/name")).unwrap_or_default();
        if !device_name.contains("Balance Board") {
            continue;
        }
        let path = std::path::Path::new("/dev/input").join(&name);
        let device = match Device::open(&path) {
            Ok(device) => device,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                permission_denied = true;
                continue;
            }
            Err(_) => continue,
        };
        if device.input_id().vendor() != 0x057e {
            continue;
        }
        let axes = [
            Axis::ABS_HAT0X,
            Axis::ABS_HAT0Y,
            Axis::ABS_HAT1X,
            Axis::ABS_HAT1Y,
        ];
        if !device
            .supported_absolute_axes()
            .is_some_and(|supported| axes.iter().all(|axis| supported.contains(*axis)))
        {
            continue;
        }
        let found_id = device
            .unique_name()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        if identity.is_some_and(|expected| expected != found_id) {
            continue;
        }
        device.set_nonblocking(true)?;
        return Ok(Box::new(Board {
            device,
            identity: found_id,
        }));
    }
    if permission_denied {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "The board is connected but access is denied. Run the included setup-linux.sh once, then reconnect the board."))
    } else {
        Err(io::Error::new(io::ErrorKind::NotFound, "No Linux Balance Board input found. Press Power on a paired board, or choose Pair in the launcher. Linux needs the hid_wiimote driver."))
    }
}

impl Source for Board {
    fn identity(&self) -> String {
        self.identity.clone()
    }
    fn poll(&mut self) -> io::Result<Option<Sample>> {
        thread::sleep(Duration::from_millis(10));
        // Drain queued events and let evdev resynchronize after dropped events.
        match self.device.fetch_events() {
            Ok(events) => for _ in events {},
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
        // Unchanged values need not generate events. Query kernel state instead
        // of declaring a motionless board disconnected. ENODEV propagates.
        let state = self.device.get_abs_state()?;
        let values = [
            Axis::ABS_HAT0X,
            Axis::ABS_HAT0Y,
            Axis::ABS_HAT1X,
            Axis::ABS_HAT1Y,
        ]
        .map(|axis| state[axis.0 as usize].value);
        let button = self.device.get_key_state()?.contains(KeyCode::BTN_SOUTH);
        Ok(Some(Sample {
            weights: weights(values),
            button,
        }))
    }
}

fn weights(values: [i32; 4]) -> CalibratedSensors {
    let [top_right, bottom_right, top_left, bottom_left] = values.map(|v| v.max(0) as f32 / 100.0);
    CalibratedSensors {
        top_right,
        bottom_right,
        top_left,
        bottom_left,
    }
}

struct Controller(VirtualDevice);
pub(super) fn output() -> io::Result<Box<dyn Output>> {
    let mut keys = AttributeSet::<KeyCode>::new();
    keys.insert(KeyCode::BTN_SOUTH);
    let mut builder = VirtualDevice::builder().map_err(|e| io::Error::new(e.kind(), format!("Cannot open /dev/uinput: {e}. Run setup-linux.sh once, then reopen the launcher.")))?
        .name("WeBalanceBored Controller").with_keys(&keys)?;
    for axis in [
        Axis::ABS_X,
        Axis::ABS_Y,
        Axis::ABS_Z,
        Axis::ABS_RX,
        Axis::ABS_RY,
        Axis::ABS_RZ,
    ] {
        builder = builder.with_absolute_axis(&UinputAbsSetup::new(
            axis,
            AbsInfo::new(0, -32767, 32767, 0, 0, 0),
        ))?;
    }
    let mut controller = Controller(builder.build()?);
    controller.neutralize()?;
    Ok(Box::new(controller))
}

fn axis(value: f32) -> i32 {
    (value.clamp(-1.0, 1.0) * 32767.0).round() as i32
}

impl Output for Controller {
    fn send(&mut self, frame: &Processed) -> io::Result<()> {
        let values = [
            frame.cog_x,
            frame.cog_y,
            frame.corner_axes[0],
            frame.corner_axes[1],
            frame.corner_axes[2],
            frame.corner_axes[3],
        ];
        let mut events: Vec<InputEvent> = [
            Axis::ABS_X,
            Axis::ABS_Y,
            Axis::ABS_Z,
            Axis::ABS_RX,
            Axis::ABS_RY,
            Axis::ABS_RZ,
        ]
        .into_iter()
        .zip(values)
        .map(|(code, value)| *AbsoluteAxisEvent::new(code, axis(value)))
        .collect();
        events.push(*KeyEvent::new(KeyCode::BTN_SOUTH, i32::from(frame.button)));
        self.0.emit(&events)
    }
    fn neutralize(&mut self) -> io::Result<()> {
        self.send(&Processed {
            cog_x: 0.0,
            cog_y: 0.0,
            cog_loaded: false,
            corner_axes: [-1.0; 4],
            total_kg: 0.0,
            button: false,
        })
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.neutralize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kernel_units_and_corner_order() {
        let w = weights([1700, 3400, 850, -1]);
        assert_eq!(
            (w.top_right, w.bottom_right, w.top_left, w.bottom_left),
            (17.0, 34.0, 8.5, 0.0)
        );
    }
    #[test]
    fn virtual_axes_are_centered_and_clamped() {
        assert_eq!(axis(0.0), 0);
        assert_eq!(axis(-2.0), -32767);
        assert_eq!(axis(2.0), 32767);
    }
}
