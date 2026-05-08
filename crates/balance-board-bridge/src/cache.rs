//! Persistent calibration cache.
//!
//! The Wiimote register-read transaction takes a noticeable fraction of
//! a second on slow Bluetooth stacks. Calibration constants are
//! per-device and stable for the life of the board, so we cache them
//! to disk after the first read and use the cached copy on subsequent
//! launches.
//!
//! # Cache invalidation
//!
//! - The cache holds 24 raw bytes. If [`balance_board_protocol::Calibration::from_eeprom`]
//!   rejects the cached bytes (monotonicity check fails), the caller
//!   should discard and re-read from the board.
//! - Single cache file regardless of which board is connected — if you
//!   swap boards, run the bridge with `--no-cache` once to refresh.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CACHE_FILENAME: &str = "calibration.bin";

/// Default cache directory for the current user, or `None` if the
/// platform doesn't expose a stable user-config location (e.g. an
/// `APPDATA`-less Windows or a `HOME`-less unix).
pub fn user_cache_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("WeBalanceBored"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config").join("we-balance-bored"))
    }
}

/// Load the cached calibration bytes from `dir`, if present.
pub fn load_from(dir: &Path) -> Option<[u8; 24]> {
    let bytes = fs::read(dir.join(CACHE_FILENAME)).ok()?;
    bytes.try_into().ok()
}

/// Save calibration bytes into `dir`. Creates `dir` if needed.
pub fn save_to(dir: &Path, bytes: &[u8; 24]) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(CACHE_FILENAME), bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tempdir(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        // Process id + label keeps each test isolated even when the
        // suite is run in parallel.
        p.push(format!("wbb_cache_test_{}_{}", std::process::id(), label));
        // Ensure clean slate — a previous failed run may have left bytes.
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn roundtrip() {
        let dir = fresh_tempdir("roundtrip");
        let bytes: [u8; 24] = std::array::from_fn(|i| i as u8 + 1);
        save_to(&dir, &bytes).unwrap();
        let loaded = load_from(&dir).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(loaded, bytes);
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = fresh_tempdir("missing");
        // No save_to — the file shouldn't exist.
        assert!(load_from(&dir).is_none());
    }

    #[test]
    fn wrong_size_file_returns_none() {
        let dir = fresh_tempdir("wrongsize");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CACHE_FILENAME), b"too short").unwrap();
        let loaded = load_from(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert!(loaded.is_none(), "expected None for wrong-size cache");
    }

    #[test]
    fn save_creates_parent_dir() {
        let cleanup_root = fresh_tempdir("nested");
        let dir = cleanup_root.join("nested").join("deeper");
        let bytes = [0xAB; 24];
        save_to(&dir, &bytes).unwrap();
        let exists = dir.join(CACHE_FILENAME).exists();
        let _ = fs::remove_dir_all(&cleanup_root);
        assert!(exists);
    }
}
