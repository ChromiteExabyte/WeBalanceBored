//! Runtime FFI to `vJoyInterface.dll`.
//!
//! We resolve the DLL with `LoadLibraryW` + `GetProcAddress` rather than
//! declaring a load-time import. The reason: a load-time import (raw-dylib)
//! aborts the process at startup if `vJoyInterface.dll` isn't on PATH,
//! before our code can produce a friendly error. Runtime loading lets us
//! return a normal `io::Error` saying "install vJoy" — and lets the binary
//! be built and run on machines that don't have vJoy at all (useful for
//! development on Linux/macOS or for shipping CI binaries).
//!
//! Only `kernel32.dll` is imported at load time; that one is always present
//! on Windows.

#![cfg(windows)]

use std::ffi::{c_void, CString};
use std::io;
use std::mem;

// --- HID Usage Page 0x01 (Generic Desktop) — vJoy axis IDs --------------

const HID_USAGE_X: u32 = 0x30;
const HID_USAGE_Y: u32 = 0x31;
const HID_USAGE_Z: u32 = 0x32;
const HID_USAGE_RX: u32 = 0x33;
const HID_USAGE_RY: u32 = 0x34;
const HID_USAGE_RZ: u32 = 0x35;

// vJoy axis range: `1..=0x8000`. Center is `0x4000`.
const AXIS_MIN: i32 = 1;
const AXIS_MAX: i32 = 0x8000;

/// One of the six vJoy generic-desktop axes we expose to Steam Input.
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub enum VJoyAxis {
    X  = HID_USAGE_X,
    Y  = HID_USAGE_Y,
    Z  = HID_USAGE_Z,
    Rx = HID_USAGE_RX,
    Ry = HID_USAGE_RY,
    Rz = HID_USAGE_RZ,
}

// --- kernel32 (always-present, load-time linked) ------------------------

type Hmodule = *mut c_void;
type Farproc = *mut c_void;

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> Hmodule;
    fn GetProcAddress(module: Hmodule, name: *const u8) -> Farproc;
    fn FreeLibrary(module: Hmodule) -> i32;
}

// --- vJoyInterface entry-point signatures -------------------------------

type FnVJoyEnabled    = unsafe extern "system" fn() -> i32;
type FnAcquireVJD     = unsafe extern "system" fn(rid: u32) -> i32;
type FnRelinquishVJD  = unsafe extern "system" fn(rid: u32);
type FnResetVJD       = unsafe extern "system" fn(rid: u32) -> i32;
type FnSetAxis        = unsafe extern "system" fn(value: i32, rid: u32, axis: u32) -> i32;
type FnSetBtn         = unsafe extern "system" fn(value: i32, rid: u32, btn: u8) -> i32;

struct VJoyApi {
    module: Hmodule,
    enabled: FnVJoyEnabled,
    acquire: FnAcquireVJD,
    relinquish: FnRelinquishVJD,
    reset: FnResetVJD,
    set_axis: FnSetAxis,
    set_btn: FnSetBtn,
}

impl VJoyApi {
    fn load() -> io::Result<Self> {
        // SAFETY: we own the wide-char buffer for the duration of the call;
        // `LoadLibraryW` reads it but doesn't store it.
        let module = unsafe {
            let name: Vec<u16> = "vJoyInterface.dll\0".encode_utf16().collect();
            LoadLibraryW(name.as_ptr())
        };
        if module.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "vJoyInterface.dll could not be loaded. Install vJoy from \
                 https://github.com/jshafer817/vJoy/releases and ensure its \
                 install dir (e.g. C:\\Program Files\\vJoy\\x64) is on PATH.",
            ));
        }

        // SAFETY: `module` is non-null and remains valid until we
        // `FreeLibrary` (in `Drop`). `GetProcAddress` reads `cname` for the
        // duration of the call only.
        let resolve = |sym: &str| -> io::Result<Farproc> {
            let cname = CString::new(sym).expect("symbol name has no NUL");
            let p = unsafe { GetProcAddress(module, cname.as_ptr() as *const u8) };
            if p.is_null() {
                unsafe { FreeLibrary(module) };
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("vJoyInterface.dll is missing export `{sym}` — wrong vJoy version?"),
                ))
            } else {
                Ok(p)
            }
        };

        // SAFETY: each function pointer is the address of a symbol we just
        // resolved; their declared signatures match vJoyInterface's ABI
        // (`extern "system"`, types verified against vjoyinterface.h).
        unsafe {
            Ok(VJoyApi {
                module,
                enabled:    mem::transmute::<Farproc, FnVJoyEnabled>(resolve("vJoyEnabled")?),
                acquire:    mem::transmute::<Farproc, FnAcquireVJD>(resolve("AcquireVJD")?),
                relinquish: mem::transmute::<Farproc, FnRelinquishVJD>(resolve("RelinquishVJD")?),
                reset:      mem::transmute::<Farproc, FnResetVJD>(resolve("ResetVJD")?),
                set_axis:   mem::transmute::<Farproc, FnSetAxis>(resolve("SetAxis")?),
                set_btn:    mem::transmute::<Farproc, FnSetBtn>(resolve("SetBtn")?),
            })
        }
    }
}

impl Drop for VJoyApi {
    fn drop(&mut self) {
        // SAFETY: `module` was obtained from a successful `LoadLibraryW`
        // and isn't shared elsewhere.
        unsafe {
            FreeLibrary(self.module);
        }
    }
}

// --- Public API ---------------------------------------------------------

/// An acquired vJoy virtual device. Released on `Drop`.
pub struct VJoyDevice {
    id: u32,
    api: VJoyApi,
}

impl VJoyDevice {
    /// Load `vJoyInterface.dll`, check the driver is enabled, and acquire
    /// device `id` (1–16; vJoy ships with device 1 by default).
    ///
    /// # Errors
    /// - [`io::ErrorKind::NotFound`] if `vJoyInterface.dll` is not installed,
    ///   the driver isn't enabled, or device `id` isn't configured.
    /// - [`io::ErrorKind::Other`] if the device is configured but already
    ///   acquired by another process.
    pub fn acquire(id: u32) -> io::Result<Self> {
        let api = VJoyApi::load()?;
        // SAFETY: function pointers were resolved by `VJoyApi::load` and
        // share `api`'s lifetime; calls follow the documented vJoy contract.
        unsafe {
            if (api.enabled)() == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "vJoy driver not enabled. Run vJoyConf and configure at least one device.",
                ));
            }
            if (api.acquire)(id) == 0 {
                return Err(io::Error::other(format!(
                    "could not acquire vJoy device {id} — already in use, or not configured"
                )));
            }
            (api.reset)(id);
        }
        Ok(VJoyDevice { id, api })
    }

    /// Set an axis from a normalized `[-1.0, +1.0]` value. Out-of-range
    /// inputs are clamped. `0.0` corresponds to vJoy's axis center.
    pub fn set_axis_normalized(&mut self, axis: VJoyAxis, value: f32) {
        let v = value.clamp(-1.0, 1.0);
        let span = (AXIS_MAX - AXIS_MIN) as f32;
        let scaled = ((v + 1.0) * 0.5 * span) as i32 + AXIS_MIN;
        // SAFETY: see `acquire`. `axis as u32` is one of the documented
        // HID Usage Page 0x01 generic-desktop axis IDs.
        unsafe {
            (self.api.set_axis)(scaled, self.id, axis as u32);
        }
    }

    /// Set a button (1-indexed). vJoy supports up to 128 buttons.
    pub fn set_button(&mut self, btn: u8, pressed: bool) {
        // SAFETY: see `acquire`.
        unsafe {
            (self.api.set_btn)(i32::from(pressed), self.id, btn);
        }
    }
}

impl Drop for VJoyDevice {
    fn drop(&mut self) {
        // SAFETY: paired with the successful `AcquireVJD` in `acquire`.
        // `api` outlives this Drop because we own it.
        unsafe {
            (self.api.relinquish)(self.id);
        }
    }
}
