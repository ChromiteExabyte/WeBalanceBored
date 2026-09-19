//! Interactive terminal entry point for packaged downloads.
fn main() {
    if let Err(error) = balance_board_bridge::cli::main(true) {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
