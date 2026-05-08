//! Smoke test: open a paired Balance Board, read its calibration, and
//! print live calibrated sensor values + center-of-gravity until Ctrl-C.
//!
//! Pair the board first via your OS Bluetooth UI, then run:
//!
//! ```pwsh
//! cargo run --release -p balance-board-io --example print_sensors
//! ```

use balance_board_io::{BalanceBoardSource, HidApiBoard};
use balance_board_protocol::Calibration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Opening Balance Board...");
    let mut board = HidApiBoard::open()?;

    eprintln!("Reading calibration block...");
    let cal_bytes = board.read_calibration_block()?;
    let cal = Calibration::from_eeprom(&cal_bytes)?;
    eprintln!("Calibration OK: {cal:?}");

    eprintln!("Streaming. Ctrl-C to stop.\n");
    eprintln!(
        "{:>8} | {:>6} {:>6} {:>6} {:>6} | {:>6} {:>6} | btn",
        "total kg", "TR", "BR", "TL", "BL", "cog x", "cog y"
    );
    loop {
        let report = board.next_report()?;
        let kg = cal.calibrate(report.sensors);
        let total = kg.total_kg();
        let cog = kg.center_of_gravity(2.0);
        let btn = if report.buttons.balance_board_button() { "DOWN" } else { "" };
        match cog {
            Some(c) => println!(
                "{total:>8.2} | {:>6.2} {:>6.2} {:>6.2} {:>6.2} | {:>+6.3} {:>+6.3} | {btn}",
                kg.top_right, kg.bottom_right, kg.top_left, kg.bottom_left, c.x, c.y
            ),
            None => println!(
                "{total:>8.2} | {:>6.2} {:>6.2} {:>6.2} {:>6.2} | {:>6} {:>6} | {btn}",
                kg.top_right, kg.bottom_right, kg.top_left, kg.bottom_left, "—", "—"
            ),
        }
    }
}
