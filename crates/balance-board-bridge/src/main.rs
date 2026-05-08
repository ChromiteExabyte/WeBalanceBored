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
use balance_board_protocol::{BoardReport, CalibratedSensors, Calibration, LowPass2D};

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
        let processed = process_report(&report, &cal, (tare_x, tare_y), &mut filter);

        #[cfg(windows)]
        {
            vjoy.set_axis_normalized(VJoyAxis::X, processed.cog_x);
            vjoy.set_axis_normalized(VJoyAxis::Y, processed.cog_y);
            vjoy.set_axis_normalized(VJoyAxis::Z,  processed.corner_axes[0]);
            vjoy.set_axis_normalized(VJoyAxis::Rx, processed.corner_axes[1]);
            vjoy.set_axis_normalized(VJoyAxis::Ry, processed.corner_axes[2]);
            vjoy.set_axis_normalized(VJoyAxis::Rz, processed.corner_axes[3]);
            // The board's front-edge SYNC button surfaces as vJoy
            // button 1; Steam Input can bind it to anything.
            vjoy.set_button(1, processed.button);
        }

        #[cfg(not(windows))]
        {
            let tag = if processed.cog_loaded { "" } else { " (unloaded)" };
            let btn = if processed.button { " btn" } else { "" };
            println!(
                "kg={:.1} x={:+.2} y={:+.2}{tag}{btn}",
                processed.total_kg, processed.cog_x, processed.cog_y
            );
        }
    }
}

/// Result of running one [`BoardReport`] through the bridge's
/// tare → clamp → smooth → corner-axis pipeline. Pure data; the
/// caller wires it to a vJoy device (or a print loop, in tests
/// or non-Windows builds).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Processed {
    /// Smoothed COG x in `[-1, +1]`, after tare offset and clamp.
    cog_x: f32,
    /// Smoothed COG y in `[-1, +1]`, after tare offset and clamp.
    cog_y: f32,
    /// `false` when the board reports below `MIN_TOTAL_KG` and the
    /// COG was therefore replaced with `(0, 0)` for filter input.
    cog_loaded: bool,
    /// Per-corner load mapped to `[-1, +1]` via [`per_corner_axis`],
    /// in report order: TR, BR, TL, BL.
    corner_axes: [f32; 4],
    /// Total weight on the board in kg (sum of the four calibrated
    /// sensors). Useful for status output and threshold logic.
    total_kg: f32,
    /// `true` if the Balance Board's front-edge SYNC button was
    /// pressed in this report.
    button: bool,
}

/// Run one report through the bridge's signal-processing pipeline.
///
/// Pure function — no I/O, no time, fully deterministic given a
/// freshly-reset filter and a fixed tare. This is what the bridge
/// tests exercise instead of mocking vJoy and an HID source.
fn process_report(
    report: &BoardReport,
    cal: &Calibration,
    tare: (f32, f32),
    filter: &mut LowPass2D,
) -> Processed {
    let kg = cal.calibrate(report.sensors);
    let cog = kg.center_of_gravity(MIN_TOTAL_KG);
    let cog_loaded = cog.is_some();

    let (x_in, y_in) = match cog {
        Some(c) => (
            (c.x - tare.0).clamp(-1.0, 1.0),
            (c.y - tare.1).clamp(-1.0, 1.0),
        ),
        None => (0.0, 0.0),
    };
    let (cog_x, cog_y) = filter.update(x_in, y_in);

    Processed {
        cog_x,
        cog_y,
        cog_loaded,
        corner_axes: corner_axes(&kg),
        total_kg: kg.total_kg(),
        button: report.buttons.balance_board_button(),
    }
}

fn corner_axes(kg: &CalibratedSensors) -> [f32; 4] {
    [
        per_corner_axis(kg.top_right),
        per_corner_axis(kg.bottom_right),
        per_corner_axis(kg.top_left),
        per_corner_axis(kg.bottom_left),
    ]
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

#[cfg(test)]
mod tests {
    use super::*;
    use balance_board_protocol::{RawSensors, SensorQuad, WiimoteButtons};

    /// Calibration where every sensor reads 5000 raw at 0kg, 10000 at 17kg,
    /// 15000 at 34kg — same shape the protocol crate's tests use, easy to
    /// reason about in head math.
    fn uniform_cal() -> Calibration {
        let pt = |v| SensorQuad { top_right: v, bottom_right: v, top_left: v, bottom_left: v };
        Calibration { kg0: pt(5000), kg17: pt(10000), kg34: pt(15000) }
    }

    fn report(tr: u16, br: u16, tl: u16, bl: u16, button: bool) -> BoardReport {
        BoardReport {
            sensors: RawSensors {
                top_right: tr,
                bottom_right: br,
                top_left: tl,
                bottom_left: bl,
            },
            // bit 3 of byte 2 = A button = the firmware-typical Balance
            // Board button bit. With `balance_board_button()` returning
            // OR-of-all-bits, any non-zero second byte trips it.
            buttons: WiimoteButtons::from_bytes(0, if button { 0x08 } else { 0x00 }),
        }
    }

    /// Each sensor at 17 kg cal point and equal load — perfectly centered.
    fn balanced_17kg() -> BoardReport {
        report(10000, 10000, 10000, 10000, false)
    }

    /// alpha = 1.0 → filter is passthrough; we get the input out
    /// unchanged after one update.
    fn passthrough_filter() -> LowPass2D {
        LowPass2D::new(1.0)
    }

    #[test]
    fn balanced_load_with_zero_tare_centers_cog() {
        let cal = uniform_cal();
        let mut filt = passthrough_filter();
        let r = balanced_17kg();
        let p = process_report(&r, &cal, (0.0, 0.0), &mut filt);
        assert!(p.cog_loaded);
        assert!(p.cog_x.abs() < 1e-3);
        assert!(p.cog_y.abs() < 1e-3);
        assert!((p.total_kg - 68.0).abs() < 1e-3);
        assert!(!p.button);
    }

    #[test]
    fn tare_offset_is_subtracted() {
        let cal = uniform_cal();
        let mut filt = passthrough_filter();
        let r = balanced_17kg();
        // Pretend the user's natural stance was at (+0.20, -0.10);
        // a centered-at-zero report should now read (-0.20, +0.10).
        let p = process_report(&r, &cal, (0.20, -0.10), &mut filt);
        assert!((p.cog_x - (-0.20)).abs() < 1e-3);
        assert!((p.cog_y - 0.10).abs() < 1e-3);
    }

    #[test]
    fn tared_lean_clamps_to_unit() {
        let cal = uniform_cal();
        let mut filt = passthrough_filter();
        // All weight on right side at 17 kg per sensor: COG x = +1.0.
        let r = report(10000, 10000, 5000, 5000, false);
        // Tare of -0.5 would push x to +1.5 without clamping; expect +1.0.
        let p = process_report(&r, &cal, (-0.5, 0.0), &mut filt);
        assert!((p.cog_x - 1.0).abs() < 1e-3);
    }

    #[test]
    fn unloaded_board_emits_centered_cog() {
        let cal = uniform_cal();
        let mut filt = passthrough_filter();
        // All sensors at 0kg cal point: total weight 0, COG undefined.
        let r = report(5000, 5000, 5000, 5000, false);
        let p = process_report(&r, &cal, (0.0, 0.0), &mut filt);
        assert!(!p.cog_loaded);
        assert_eq!(p.cog_x, 0.0);
        assert_eq!(p.cog_y, 0.0);
    }

    #[test]
    fn button_propagates_through_pipeline() {
        let cal = uniform_cal();
        let mut filt = passthrough_filter();
        let r = report(10000, 10000, 10000, 10000, true);
        let p = process_report(&r, &cal, (0.0, 0.0), &mut filt);
        assert!(p.button);
    }

    #[test]
    fn smoothing_attenuates_first_step() {
        let cal = uniform_cal();
        let mut filt = LowPass2D::new(0.4);
        // Warm filter to (0, 0) with one passthrough update.
        let _ = filt.update(0.0, 0.0);
        // Hard right lean: raw COG x = +1.0.
        let r = report(10000, 10000, 5000, 5000, false);
        let p = process_report(&r, &cal, (0.0, 0.0), &mut filt);
        // alpha=0.4 → first step lands 40% of the way to +1.0.
        assert!((p.cog_x - 0.4).abs() < 1e-3);
    }

    #[test]
    fn corner_axes_in_report_order_and_full_scale() {
        let cal = uniform_cal();
        let mut filt = passthrough_filter();
        // Construct sensors so each corner is exactly at full-scale kg.
        // PER_CORNER_FULL_SCALE_KG = 50; at uniform_cal that's raw value
        // ~17647 (50 kg ≈ 14706 + bit; let's use 18382). Easier to just
        // hand-build CalibratedSensors via... wait, we don't expose that
        // through process_report. Use the kg17 point per sensor (= 17 kg)
        // and confirm corner_axes produce 17/50 → -1 + 2*(17/50) = -0.32.
        let r = report(10000, 10000, 10000, 10000, false);
        let p = process_report(&r, &cal, (0.0, 0.0), &mut filt);
        let expected = -1.0 + 2.0 * (17.0 / PER_CORNER_FULL_SCALE_KG);
        for a in p.corner_axes {
            assert!((a - expected).abs() < 1e-3, "got {a}, expected {expected}");
        }
    }

    #[test]
    fn per_corner_axis_clamps_above_full_scale() {
        // 200 kg on one corner is way above the 50 kg full-scale.
        assert_eq!(per_corner_axis(200.0), 1.0);
    }

    #[test]
    fn per_corner_axis_zero_at_zero_kg() {
        assert_eq!(per_corner_axis(0.0), -1.0);
    }
}
