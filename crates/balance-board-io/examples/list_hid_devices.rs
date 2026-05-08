//! Diagnostic: dump every HID device hidapi can see, with all the
//! identifying fields we care about for Balance Board discovery.
//!
//! Use this when [`balance_board_io::HidApiBoard::open`] returns
//! `NotFound` even though Windows clearly shows `Nintendo RVL-WBC-01`
//! under Bluetooth Settings. The output tells you exactly which fields
//! hidapi populated on this machine — most often the difference is a
//! generic `HID-compliant game controller` product string instead of
//! the Bluetooth-level name.
//!
//! ```pwsh
//! cargo run -p balance-board-io --example list_hid_devices
//! ```

use hidapi::HidApi;

const NINTENDO_VID: u16 = 0x057E;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = HidApi::new()?;
    let mut total = 0usize;
    let mut nintendo = 0usize;

    for info in api.device_list() {
        total += 1;
        let is_nintendo = info.vendor_id() == NINTENDO_VID;
        if is_nintendo {
            nintendo += 1;
            println!("--- device {total} (NINTENDO — likely candidate) ---");
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
    if nintendo == 0 {
        println!(
            "No Nintendo devices visible to hidapi. Either the Balance Board \
             isn't paired yet, or it's paired but Windows hasn't surfaced it as \
             a usable HID interface (try unpair + repair, or check Bluetooth \
             Settings to confirm it shows as connected)."
        );
    }
    Ok(())
}
