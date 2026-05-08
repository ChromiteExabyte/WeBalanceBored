//! WeBalanceBored — Wii Balance Board → vJoy bridge for Steam Input.
//!
//! End-to-end flow:
//! 1. Open the Balance Board via [`balance_board_io::HidApiBoard`].
//! 2. Read the 24-byte EEPROM calibration block.
//! 3. (Windows) acquire vJoy device 1.
//! 4. For each sensor report: calibrate, compute center-of-gravity, push
//!    to vJoy axes. Map per-corner kg loads to Z/Rx/Ry/Rz so Steam Input
//!    can build richer per-game bindings if desired.
//!
//! Map vJoy → game in Steam Input. Recommended Superflight mapping:
//! vJoy X → right-stick X, vJoy Y → right-stick Y, with a small radial
//! deadzone in Steam Input.

use balance_board_io::{BalanceBoardSource, HidApiBoard};
use balance_board_protocol::Calibration;

#[cfg(windows)]
mod vjoy;

#[cfg(windows)]
use vjoy::{VJoyAxis, VJoyDevice};

/// Below this load (kilograms across the whole board) we treat the board
/// as unloaded and don't push center-of-gravity values to vJoy.
const MIN_TOTAL_KG: f32 = 2.0;

/// vJoy device ID to acquire. vJoy supports IDs 1–16; 1 is the default
/// every fresh install ships with. Make this configurable later.
#[cfg(windows)]
const VJOY_DEVICE_ID: u32 = 1;

/// Per-corner load above which we report axis-max on Z/Rx/Ry/Rz.
/// 50 kg per corner is comfortably above the load a single foot puts on
/// one corner during normal use, but well below the board's published
/// 150 kg limit, so the upper segment has headroom.
const PER_CORNER_FULL_SCALE_KG: f32 = 50.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("WeBalanceBored — opening Balance Board...");
    let mut board = HidApiBoard::open()?;

    eprintln!("Reading calibration...");
    let cal_bytes = board.read_calibration_block()?;
    let cal = Calibration::from_eeprom(&cal_bytes)?;

    #[cfg(windows)]
    let mut vjoy = {
        eprintln!("Acquiring vJoy device {VJOY_DEVICE_ID}...");
        VJoyDevice::acquire(VJOY_DEVICE_ID)?
    };

    #[cfg(not(windows))]
    eprintln!("(non-Windows build — vJoy disabled, running in print-only mode)");

    eprintln!("Streaming. Ctrl-C to stop.\n");
    loop {
        let raw = board.next_report()?;
        let kg = cal.calibrate(raw);
        let cog = kg.center_of_gravity(MIN_TOTAL_KG);

        #[cfg(windows)]
        {
            // X/Y always update — even when the board is unloaded we want
            // to recenter so the game sees a neutral input.
            let (x, y) = cog.map_or((0.0, 0.0), |c| (c.x, c.y));
            vjoy.set_axis_normalized(VJoyAxis::X, x);
            vjoy.set_axis_normalized(VJoyAxis::Y, y);

            // Z/Rx/Ry/Rz mirror the four per-corner kg loads in `[-1, +1]`.
            // Steam Input can ignore these or use them for advanced bindings
            // (e.g. mapping bottom-corner pressure to a brake action).
            vjoy.set_axis_normalized(VJoyAxis::Z,  per_corner_axis(kg.top_right));
            vjoy.set_axis_normalized(VJoyAxis::Rx, per_corner_axis(kg.bottom_right));
            vjoy.set_axis_normalized(VJoyAxis::Ry, per_corner_axis(kg.top_left));
            vjoy.set_axis_normalized(VJoyAxis::Rz, per_corner_axis(kg.bottom_left));
        }

        #[cfg(not(windows))]
        {
            match cog {
                Some(c) => println!("kg={:.1} x={:+.2} y={:+.2}", kg.total_kg(), c.x, c.y),
                None => println!("kg={:.1} (unloaded)", kg.total_kg()),
            }
        }
    }
}

/// Map a per-corner kilogram load to an axis value in `[-1.0, +1.0]`,
/// linear from 0 kg → -1.0 up to [`PER_CORNER_FULL_SCALE_KG`] → +1.0.
fn per_corner_axis(kg: f32) -> f32 {
    let t = (kg / PER_CORNER_FULL_SCALE_KG).clamp(0.0, 1.0);
    t * 2.0 - 1.0
}
