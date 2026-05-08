//! WeBalanceBored — Wii Balance Board → vJoy bridge for Steam Input.
//!
//! End-to-end flow:
//! 1. Open the Balance Board via [`balance_board_io::HidApiBoard`].
//! 2. Read the 24-byte EEPROM calibration block.
//! 3. (Windows) acquire vJoy device 1.
//! 4. Capture **tare**: average the user's natural-stance COG over a
//!    short warm-up so a relaxed stand reads as `(0, 0)` to the game.
//! 5. For each sensor report: calibrate, compute center-of-gravity,
//!    subtract tare, **low-pass filter**, push to vJoy axes. Map
//!    per-corner kg loads to Z/Rx/Ry/Rz for advanced bindings.
//!
//! Map vJoy → game in Steam Input. Recommended Superflight mapping:
//! vJoy X → right-stick X, vJoy Y → right-stick Y, plus a small radial
//! deadzone in Steam Input.
//!
//! # CLI flags
//!
//! - `--no-tare`   — skip the warm-up; assume the board is already centered.
//! - `--no-smooth` — disable the low-pass filter; raw COG goes straight to vJoy.

use balance_board_io::{BalanceBoardSource, HidApiBoard};
use balance_board_protocol::{Calibration, LowPass2D};

mod cache;

#[cfg(windows)]
mod vjoy;

#[cfg(windows)]
use vjoy::{VJoyAxis, VJoyDevice};

/// Below this load (kilograms across the whole board) we treat the board
/// as unloaded and don't push center-of-gravity values to vJoy.
const MIN_TOTAL_KG: f32 = 2.0;

/// vJoy device ID to acquire. vJoy supports IDs 1–16; 1 is the default
/// every fresh install ships with.
#[cfg(windows)]
const VJOY_DEVICE_ID: u32 = 1;

/// Per-corner load mapped to axis-max on Z/Rx/Ry/Rz. 50 kg/corner is
/// comfortably above normal use but well below the board's 150 kg limit.
const PER_CORNER_FULL_SCALE_KG: f32 = 50.0;

/// Frames averaged for the tare offset. ~100 Hz reports → ~1 s.
const TARE_FRAMES: usize = 100;

/// Low-pass smoothing factor for COG. Higher = more responsive,
/// lower = smoother. 0.4 at 100 Hz reaches ~95% of a step in ~50 ms.
const COG_ALPHA: f32 = 0.4;

struct Args {
    no_tare: bool,
    no_smooth: bool,
    no_cache: bool,
}

impl Args {
    fn from_env() -> Self {
        let mut args = Args { no_tare: false, no_smooth: false, no_cache: false };
        for a in std::env::args().skip(1) {
            match a.as_str() {
                "--no-tare" => args.no_tare = true,
                "--no-smooth" => args.no_smooth = true,
                "--no-cache" => args.no_cache = true,
                "-h" | "--help" => {
                    eprintln!(
                        "Usage: balance-board-bridge [--no-tare] [--no-smooth] [--no-cache]\n\n\
                         --no-tare    Skip the warm-up that calibrates a centered stance.\n\
                         --no-smooth  Disable the low-pass filter (raw COG to vJoy).\n\
                         --no-cache   Re-read calibration from the board, ignore the on-disk cache.\n\
                         \n\
                         Cache location: {}",
                        cache::user_cache_dir()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "(no user-config dir on this platform)".into()),
                    );
                    std::process::exit(0);
                }
                other => {
                    eprintln!("Unknown argument: {other}\nTry --help.");
                    std::process::exit(2);
                }
            }
        }
        args
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::from_env();

    eprintln!("WeBalanceBored — opening Balance Board...");
    let mut board = HidApiBoard::open()?;

    let cal_bytes = load_or_read_calibration(&mut board, args.no_cache)?;
    let cal = Calibration::from_eeprom(&cal_bytes)?;

    #[cfg(windows)]
    let mut vjoy = {
        eprintln!("Acquiring vJoy device {VJOY_DEVICE_ID}...");
        VJoyDevice::acquire(VJOY_DEVICE_ID)?
    };
    #[cfg(not(windows))]
    eprintln!("(non-Windows build — vJoy disabled, running in print-only mode)");

    let (tare_x, tare_y) = if args.no_tare {
        eprintln!("Tare skipped (--no-tare).");
        (0.0, 0.0)
    } else {
        let (tx, ty) = capture_tare(&mut board, &cal)?;
        eprintln!("Tare captured: cog_x={tx:+.3} cog_y={ty:+.3}");
        (tx, ty)
    };

    // Alpha = 1.0 is mathematical passthrough — the filter compiles away
    // to "return input unchanged" without a separate code path.
    let alpha = if args.no_smooth { 1.0 } else { COG_ALPHA };
    let mut filter = LowPass2D::new(alpha);

    eprintln!("\nStreaming. Ctrl-C to stop.");
    loop {
        let report = board.next_report()?;
        let kg = cal.calibrate(report.sensors);
        let cog = kg.center_of_gravity(MIN_TOTAL_KG);
        let button = report.buttons.balance_board_button();

        // Tare → clamp → smooth. When the board reads as unloaded we feed
        // (0, 0) so the filter walks back to center instead of holding the
        // last lean. Players can lift their feet without "joystick stuck".
        let (x_in, y_in) = match cog {
            Some(c) => (
                (c.x - tare_x).clamp(-1.0, 1.0),
                (c.y - tare_y).clamp(-1.0, 1.0),
            ),
            None => (0.0, 0.0),
        };
        let (x, y) = filter.update(x_in, y_in);

        #[cfg(windows)]
        {
            vjoy.set_axis_normalized(VJoyAxis::X, x);
            vjoy.set_axis_normalized(VJoyAxis::Y, y);
            vjoy.set_axis_normalized(VJoyAxis::Z,  per_corner_axis(kg.top_right));
            vjoy.set_axis_normalized(VJoyAxis::Rx, per_corner_axis(kg.bottom_right));
            vjoy.set_axis_normalized(VJoyAxis::Ry, per_corner_axis(kg.top_left));
            vjoy.set_axis_normalized(VJoyAxis::Rz, per_corner_axis(kg.bottom_left));
            // The board's front-edge SYNC button surfaces as vJoy
            // button 1; Steam Input can bind it to anything.
            vjoy.set_button(1, button);
        }

        #[cfg(not(windows))]
        {
            let tag = if cog.is_some() { "" } else { " (unloaded)" };
            let btn = if button { " btn" } else { "" };
            println!("kg={:.1} x={x:+.2} y={y:+.2}{tag}{btn}", kg.total_kg());
        }
    }
}

/// Try the on-disk cache first; on miss, corrupt cache, or
/// `--no-cache`, do the live EEPROM read and update the cache.
fn load_or_read_calibration(
    board: &mut HidApiBoard,
    no_cache: bool,
) -> Result<[u8; 24], Box<dyn std::error::Error>> {
    let cache_dir = cache::user_cache_dir();

    if !no_cache {
        if let Some(dir) = cache_dir.as_deref() {
            if let Some(bytes) = cache::load_from(dir) {
                // Sanity-check: a stale/corrupt cache shouldn't silently
                // produce wrong calibration. Validate via the protocol
                // crate's monotonicity check before trusting.
                if Calibration::from_eeprom(&bytes).is_ok() {
                    eprintln!("Calibration loaded from cache ({}).", dir.display());
                    return Ok(bytes);
                }
                eprintln!("Cached calibration failed validation; re-reading from board.");
            }
        }
    }

    eprintln!("Reading calibration from board...");
    let bytes = board.read_calibration_block()?;

    if let Some(dir) = cache_dir.as_deref() {
        match cache::save_to(dir, &bytes) {
            Ok(()) => eprintln!("Cached calibration to {}.", dir.display()),
            Err(e) => eprintln!("Could not write calibration cache: {e}"),
        }
    }

    Ok(bytes)
}

/// Block until the board is loaded, then average COG over
/// [`TARE_FRAMES`] reports to capture the user's natural-stance offset.
fn capture_tare(
    board: &mut HidApiBoard,
    cal: &Calibration,
) -> Result<(f32, f32), Box<dyn std::error::Error>> {
    eprintln!(
        "Tare: stand centered (don't lean). Averaging {TARE_FRAMES} frames once \
         you're on the board — about a second."
    );

    // Phase 1: wait for someone to actually step on the board.
    loop {
        let report = board.next_report()?;
        let kg = cal.calibrate(report.sensors);
        if kg.center_of_gravity(MIN_TOTAL_KG).is_some() {
            break;
        }
    }

    // Phase 2: average COG over the window.
    let mut sum_x = 0.0f32;
    let mut sum_y = 0.0f32;
    let mut count = 0usize;
    while count < TARE_FRAMES {
        let report = board.next_report()?;
        let kg = cal.calibrate(report.sensors);
        if let Some(c) = kg.center_of_gravity(MIN_TOTAL_KG) {
            sum_x += c.x;
            sum_y += c.y;
            count += 1;
        }
    }
    Ok((sum_x / count as f32, sum_y / count as f32))
}

/// Map a per-corner kilogram load to an axis value in `[-1.0, +1.0]`,
/// linear from 0 kg → -1.0 up to [`PER_CORNER_FULL_SCALE_KG`] → +1.0.
fn per_corner_axis(kg: f32) -> f32 {
    let t = (kg / PER_CORNER_FULL_SCALE_KG).clamp(0.0, 1.0);
    t * 2.0 - 1.0
}
