//! Command-line controller bridge.
fn main() {
    if let Err(error) = balance_board_bridge::cli::main(false) {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
