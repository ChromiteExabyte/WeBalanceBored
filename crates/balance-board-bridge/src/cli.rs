//! Terminal launcher shared by the packaged app and developer CLI.
use crate::{Config, Control, Phase};
use std::{
    io::{self, Write},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

/// Run the terminal interface. The launcher shows a menu when given no flags.
pub fn main(launcher: bool) -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("WeBalanceBored — Windows / Linux\n\nwe-balance-bored                 Open the launcher menu\nwe-balance-bored --monitor       Live weight, no controller driver needed\nwe-balance-bored --gamepad       Lean-to-controller bridge\nwe-balance-bored --pair          Pair the board\nwe-balance-bored --doctor        Check setup\n\nOptions: --no-tare, --no-smooth, --no-cache, --verbose\nCtrl+C disconnects and releases the controller.\n");
        return Ok(());
    }
    let mut mode = if launcher { "--monitor" } else { "--gamepad" }.to_owned();
    let mut chosen_mode = false;
    let mut config = Config::default();
    for arg in &args {
        match arg.as_str() {
            "--monitor" | "--gamepad" | "--pair" | "--doctor" => {
                if chosen_mode {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Choose only one mode.",
                    ));
                }
                chosen_mode = true;
                mode.clone_from(arg);
            }
            "--no-tare" => config.no_tare = true,
            "--no-smooth" => config.no_smooth = true,
            "--no-cache" => config.no_cache = true,
            "--verbose" | "-v" => {}
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Unknown option {other}. Use --help."),
                ))
            }
        }
    }
    if launcher && args.is_empty() {
        loop {
            println!("\nWeBalanceBored\n==============\n1  Connect and view live weight\n2  Play — start controller output\n3  Pair a board\n4  Check setup\nQ  Quit\n");
            match prompt("Choose: ")?.as_str() {
                "1" => {
                    mode = "--monitor".into();
                    break;
                }
                "2" => {
                    mode = "--gamepad".into();
                    break;
                }
                "3" => {
                    if let Err(error) = pair() {
                        eprintln!("Pairing: {error}");
                    }
                }
                "4" => doctor(),
                "q" | "Q" => return Ok(()),
                _ => println!("Enter 1, 2, 3, 4, or Q."),
            }
        }
    }
    match mode.as_str() {
        "--pair" => return pair(),
        "--doctor" => {
            doctor();
            return Ok(());
        }
        _ => {}
    }
    config.gamepad = mode == "--gamepad";
    // Monitoring should show measurements immediately, even with no one on
    // the board; centering is relevant only for game output.
    if !config.gamepad {
        config.no_tare = true;
    }
    println!(
        "\n{}\nWake a paired board with Power. Ctrl+C to disconnect.\nTo zero weight: leave the board empty, tap and release its front button, then wait for confirmation.\n",
        if config.gamepad {
            "Controller mode — stand centered when connected."
        } else {
            "Live weight — controller driver not required."
        }
    );
    let control = Arc::new(Control::default());
    let signal = control.clone();
    ctrlc::set_handler(move || signal.stop()).map_err(io::Error::other)?;
    let mut previous_message = String::new();
    let mut previous_phase = Phase::Stopped;
    let mut last_print = Instant::now() - Duration::from_secs(1);
    crate::run(config, &control, |status| {
        if status.message != previous_message || status.phase != previous_phase {
            println!("\n{}", status.message);
            previous_message = status.message.clone();
            previous_phase = status.phase;
        }
        if matches!(status.phase, Phase::Live | Phase::Centering)
            && last_print.elapsed() >= Duration::from_millis(200)
        {
            print!("\r{:6.1} kg  lean {:+.2}, {:+.2}  corners TR {:5.1} BR {:5.1} TL {:5.1} BL {:5.1}    ", status.total_kg, status.lean[0], status.lean[1], status.corners[0], status.corners[1], status.corners[2], status.corners[3]);
            let _ = io::stdout().flush();
            last_print = Instant::now();
        }
    })?;
    println!();
    Ok(())
}

fn prompt(message: &str) -> io::Result<String> {
    print!("{message}");
    io::stdout().flush()?;
    let mut value = String::new();
    if io::stdin().read_line(&mut value)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "No terminal input. Use --monitor, --gamepad, or --doctor for noninteractive use.",
        ));
    }
    Ok(value.trim().to_owned())
}

fn doctor() {
    println!("\nSetup check (does not change Bluetooth settings)");
    #[cfg(windows)]
    {
        println!("Windows: monitor mode needs Bluetooth only. Game mode needs vJoy device 1, six axes (X/Y/Z/Rx/Ry/Rz), and button 1.");
        match crate::vjoy::VJoyDevice::check_available() {
            Ok(()) => println!(
                "vJoy: driver available (axes and button are checked when game mode starts)."
            ),
            Err(error) => println!("vJoy: {error}"),
        }
        let helper = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("balance-board-pair.exe")));
        println!(
            "Pairing helper: {}",
            if helper.is_some_and(|path| path.is_file()) {
                "available"
            } else {
                "missing — extract the complete download"
            }
        );
    }
    #[cfg(target_os = "linux")]
    {
        println!(
            "Linux: monitor mode uses the kernel hid_wiimote driver. Game mode uses /dev/uinput."
        );
        match Command::new("bluetoothctl").arg("--version").status() {
            Ok(status) if status.success() => {}
            _ => {
                println!("Install BlueZ (bluetoothctl) using your distribution's package manager.")
            }
        }
        println!(
            "Input access setup: {}",
            if std::path::Path::new("/etc/udev/rules.d/70-we-balance-bored.rules").exists() {
                "rules installed"
            } else {
                "run setup-linux.sh once"
            }
        );
        println!(
            "uinput: {}",
            if std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/uinput")
                .is_ok()
            {
                "writable"
            } else {
                "unavailable — run setup-linux.sh and reopen the terminal"
            }
        );
    }
    // Opening a source performs the sensor handshake, but does not pair,
    // unpair, or acquire a virtual gamepad.
    match crate::backend::open(None, false) {
        Ok(_) => {
            println!("Board: opened successfully. Select live weight to verify changing readings.")
        }
        Err(error) => println!("Board: {error}"),
    }
}

fn pair() -> io::Result<()> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe()?;
        let helper = exe
            .parent()
            .ok_or_else(|| io::Error::other("Cannot locate application folder"))?
            .join("balance-board-pair.exe");
        println!("Press the red SYNC button inside the battery compartment, then press Enter.");
        prompt("")?;
        let status = Command::new(helper).status()?;
        if !status.success() {
            return Err(io::Error::other(
                "Pairing did not complete. Try again while the blue light is flashing.",
            ));
        }
        println!("Pairing tool completed. Select live weight to verify the connection.");
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        pair_linux()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Pairing supports Windows and Linux.",
        ))
    }
}

#[cfg(target_os = "linux")]
fn bluetooth(args: &[&str]) -> io::Result<std::process::Output> {
    Command::new("bluetoothctl")
        .env("LC_ALL", "C")
        .args(["--timeout", "20"])
        .args(args)
        .output()
}

#[cfg(target_os = "linux")]
fn pair_linux() -> io::Result<()> {
    println!("Press the red SYNC button, then Enter. Scanning takes about 12 seconds.");
    prompt("")?;
    let scan = Command::new("bluetoothctl")
        .env("LC_ALL", "C")
        .args(["--timeout", "12", "scan", "on"])
        .status()?;
    if !scan.success() {
        return Err(io::Error::other(
            "Bluetooth scan failed. Check that Bluetooth is enabled and BlueZ is running.",
        ));
    }
    let devices = bluetooth(&["devices"])?;
    let devices = String::from_utf8_lossy(&devices.stdout);
    let boards = parse_boards(&devices);
    if boards.is_empty() {
        return Err(io::Error::other(
            "No Balance Board found. Press SYNC and try again.",
        ));
    }
    for (index, address) in boards.iter().enumerate() {
        println!("{}  Nintendo RVL-WBC-01  {address}", index + 1);
    }
    let selected = if boards.len() == 1 {
        0
    } else {
        prompt("Board number: ")?
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .filter(|n| *n < boards.len())
            .ok_or_else(|| io::Error::other("Invalid board number"))?
    };
    let address = &boards[selected];
    let info = bluetooth(&["info", address])?;
    if !String::from_utf8_lossy(&info.stdout)
        .lines()
        .any(|line| line.trim() == "Paired: yes")
    {
        let result = bluetooth(&["pair", address])?;
        print!("{}", String::from_utf8_lossy(&result.stdout));
        if !result.status.success() {
            return Err(io::Error::other("BlueZ could not pair the board. Press SYNC and retry. See docs/linux.md for Bluetooth compatibility notes."));
        }
    }
    for action in ["trust", "connect"] {
        let result = bluetooth(&[action, address])?;
        print!("{}", String::from_utf8_lossy(&result.stdout));
        if !result.status.success() {
            return Err(io::Error::other(format!(
                "Bluetooth {action} failed. Wake the board and retry."
            )));
        }
    }
    println!("Bluetooth steps completed. Select live weight to verify sensor input.");
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn parse_boards(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            if parts.next()? != "Device" {
                return None;
            }
            let address = parts.next()?;
            let bytes: Vec<_> = address.split(':').collect();
            if bytes.len() != 6
                || !bytes
                    .iter()
                    .all(|b| b.len() == 2 && b.bytes().all(|c| c.is_ascii_hexdigit()))
            {
                return None;
            }
            (parts.collect::<Vec<_>>().join(" ") == "Nintendo RVL-WBC-01")
                .then(|| address.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairs_only_named_boards_with_valid_addresses() {
        let result = parse_boards("Device AA:BB:CC:DD:EE:FF Nintendo RVL-WBC-01\nDevice 11:22:33:44:55:66 Nintendo RVL-CNT-01\nDevice --bad Nintendo RVL-WBC-01\n[NEW] Device 11:22:33:44:55:66 Nintendo RVL-WBC-01\n");
        assert_eq!(result, ["AA:BB:CC:DD:EE:FF"]);
    }
}
