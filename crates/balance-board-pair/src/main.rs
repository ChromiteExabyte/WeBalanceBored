//! `balance-board-pair` — auto-pair the Balance Board on Windows.

use std::time::Duration;

const HELP: &str = "\
Usage: balance-board-pair [--scan | --forget | --help]

  (no flags)    Default. Scan for nearby Wii devices, find the Balance
                Board, pair it (computing the special Wii PIN), and
                enable the HID service so Windows treats it as a
                normal game controller.

  --scan        List nearby Wii-family Bluetooth devices and exit.
                Doesn't pair anything. Useful for sanity-checking that
                the board is in pairing mode (press SYNC inside the
                battery cover).

  --forget      Unpair every Balance Board currently known to Windows.
                Useful if the device cache is in a bad state.

  --help, -h    Show this help.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("{HELP}");
        return;
    }

    #[cfg(not(windows))]
    {
        eprintln!(
            "balance-board-pair is Windows-only. On Linux use bluetoothctl, \
             on macOS use blueutil — both will pair the board correctly when \
             SYNC is pressed.\n\nThis stub binary will exit now."
        );
        std::process::exit(2);
    }

    #[cfg(windows)]
    if let Err(e) = run(&args) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use balance_board_pair::pin::{format_bd_addr, format_pin, wii_pin_for_address};
    use balance_board_pair::{forget_all_balance_boards, pair_first, scan};

    if args.iter().any(|a| a == "--scan") {
        eprintln!("Scanning (~10s). Press SYNC on any unpaired devices you want to see.");
        let devices = scan(Duration::from_secs(10))?;
        if devices.is_empty() {
            println!("No Wii-family devices found nearby.");
            return Ok(());
        }
        println!(
            "{:<24}  {:<17}  {:<17}  paired  conn  remem",
            "name", "address", "wii pin"
        );
        for d in &devices {
            println!(
                "{name:<24}  {addr:<17}  {pin:<17}  {p:<6}  {c:<4}  {r:<5}",
                name = d.name,
                addr = format_bd_addr(d.address),
                pin = format_pin(wii_pin_for_address(d.address)),
                p = if d.authenticated { "yes" } else { "no" },
                c = if d.connected { "yes" } else { "no" },
                r = if d.remembered { "yes" } else { "no" },
            );
        }
        return Ok(());
    }

    if args.iter().any(|a| a == "--forget") {
        let n = forget_all_balance_boards()?;
        eprintln!("Removed {n} Balance Board(s).");
        return Ok(());
    }

    if !args.is_empty() {
        eprintln!("Unknown argument(s): {args:?}\n\n{HELP}");
        std::process::exit(2);
    }

    eprintln!(
        "Press SYNC inside the battery cover, then waiting up to 20s for the \
         board to appear..."
    );
    let result = pair_first(Duration::from_secs(20))?;
    if result.already_paired {
        eprintln!(
            "{name} ({addr}) was already paired; HID service re-enabled.",
            name = result.name,
            addr = format_bd_addr(result.address),
        );
    } else {
        eprintln!(
            "{name} ({addr}) paired successfully.",
            name = result.name,
            addr = format_bd_addr(result.address),
        );
    }
    eprintln!("\nNext: cargo run --release -p balance-board-bridge");
    Ok(())
}

#[cfg(windows)]
use balance_board_pair as _; // keep the lib in scope when run() is cfg-gated out
