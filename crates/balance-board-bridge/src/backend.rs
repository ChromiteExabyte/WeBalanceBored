use crate::{Output, Source};
use std::io;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

pub(crate) fn open(identity: Option<&str>, no_cache: bool) -> io::Result<Box<dyn Source>> {
    #[cfg(windows)]
    {
        windows::open(identity, no_cache)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = no_cache;
        linux::open(identity)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (identity, no_cache);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "This app supports Windows and Linux.",
        ))
    }
}

pub(crate) fn output() -> io::Result<Box<dyn Output>> {
    #[cfg(windows)]
    {
        windows::output()
    }
    #[cfg(target_os = "linux")]
    {
        linux::output()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Game output supports Windows and Linux.",
        ))
    }
}
