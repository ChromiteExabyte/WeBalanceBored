//! Diagnostic: dump every HID device hidapi can see, with all the
//! identifying fields we care about for Balance Board discovery.
//!
//! Use this when [`balance_board_io::HidApiBoard::open`] returns
//! `NotFound` even though Windows clearly shows `Nintendo RVL-WBC-01`
//! under Bluetooth Settings. The output shows which fields hidapi
//! populated; Windows' friendly names alone do not establish those values.
//! A matching VID/PID identifies a candidate, not a confirmed Balance Board.
//!
//! ```pwsh
//! cargo run -p balance-board-io --example list_hid_devices
//! ```

use hidapi::HidApi;

const NINTENDO_VID: u16 = 0x057E;
const WII_PID: u16 = 0x0306;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = HidApi::new()?;
    let mut total = 0usize;
    let mut nintendo = 0usize;
    let mut candidates = 0usize;

    for info in api.device_list() {
        total += 1;
        let is_nintendo = info.vendor_id() == NINTENDO_VID;
        if is_nintendo {
            nintendo += 1;
        }
        if is_nintendo && info.product_id() == WII_PID {
            candidates += 1;
            println!("--- device {total} (Wii Remote / Balance Board candidate) ---");
        } else if is_nintendo {
            println!("--- device {total} (Nintendo, different product ID) ---");
        } else {
            println!("--- device {total} ---");
        }
        println!("  path:           {:?}", info.path());
        println!("  vendor:         0x{:04X}", info.vendor_id());
        println!("  product:        0x{:04X}", info.product_id());
        println!("  manufacturer:   {:?}", info.manufacturer_string());
        println!("  product string: {:?}", info.product_string());
        println!("  serial:         {:?}", info.serial_number());
        println!("  release:        0x{:04X}", info.release_number());
        println!("  usage page:     0x{:04X}", info.usage_page());
        println!("  usage:          0x{:04X}", info.usage());
        println!("  interface #:    {}", info.interface_number());
    }

    println!();
    println!("{total} HID device(s) total; {nintendo} from Nintendo (VID 0x057E).");
    println!("{candidates} Wii Remote / Balance Board candidate(s) (VID 0x057E, PID 0x0306).");
    if candidates == 0 {
        println!(
            "No matching HID candidate is visible. Wake the board and compare this \
             output with Windows' device list. Pairing status alone does not confirm \
             a usable HID interface; see docs/troubleshooting.md."
        );
    } else {
        println!(
            "These IDs are shared with Wii Remotes; a match does not confirm board identity \
             or working sensor reports. Next: \
             cargo run --release --locked -p balance-board-io --example print_sensors"
        );
    }
    Ok(())
}
