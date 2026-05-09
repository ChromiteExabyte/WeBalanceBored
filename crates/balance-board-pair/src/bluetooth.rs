//! Windows Bluetooth glue: scan, pair, enable HID service, forget.
//!
//! This module is the only place that talks to `BluetoothAPIs.dll`.
//! Everything else in the crate (PIN math, CLI) is portable.
//!
//! # Threading
//!
//! Pairing involves a system-managed callback thread:
//! [`BluetoothRegisterForAuthenticationEx`] takes a function pointer
//! that the OS calls when the remote device challenges us. The
//! callback's job is to send back the binary Wii PIN via
//! [`BluetoothSendAuthenticationResponseEx`]. We share the PIN with
//! the callback through `pvParam`, a context pointer the OS passes
//! through unchanged.

#![cfg(windows)]
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::io;
use std::mem;
use std::ptr;
use std::time::Duration;

use windows_sys::Win32::Devices::Bluetooth::*;
use windows_sys::Win32::Foundation::*;

use crate::pin::{wii_pin_for_address, WII_PIN_LEN};

/// One Wii-family device returned by [`scan`].
#[derive(Debug, Clone)]
pub struct WiiDevice {
    /// 6-byte Bluetooth address in Win32 `rgBytes` order (little-endian
    /// by host convention; equal to the Wii PIN).
    pub address: [u8; 6],
    /// Friendly name from the device, e.g. `Nintendo RVL-WBC-01`.
    pub name: String,
    /// `true` when Windows considers the device already paired.
    pub authenticated: bool,
    /// `true` when Windows is currently connected to the device.
    pub connected: bool,
    /// `true` when Windows has the device in its known/remembered list.
    pub remembered: bool,
}

impl WiiDevice {
    /// Is this specifically a Balance Board (vs. a Wiimote)?
    #[must_use]
    pub fn is_balance_board(&self) -> bool {
        self.name.starts_with("Nintendo RVL-WBC-01")
    }
}

const WII_NAME_PREFIXES: &[&str] = &[
    "Nintendo RVL-WBC-01", // Balance Board
    "Nintendo RVL-CNT-01", // Wiimote / Wiimote Plus
];

fn is_wii_name(name: &str) -> bool {
    WII_NAME_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Scan for nearby Wii-family Bluetooth devices, including ones that
/// are already paired. Issues a fresh inquiry; the SYNC button on the
/// board must be active for an unpaired board to respond.
///
/// `timeout` is rounded up to the nearest 1.28-second unit (Windows'
/// inquiry quantum); minimum 1 unit, maximum 48 (~61 s).
pub fn scan(timeout: Duration) -> io::Result<Vec<WiiDevice>> {
    let timeout_units = ((timeout.as_secs_f32() / 1.28).ceil() as u8).clamp(1, 48);

    let mut params: BLUETOOTH_DEVICE_SEARCH_PARAMS = unsafe { mem::zeroed() };
    params.dwSize = mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32;
    params.fReturnAuthenticated = 1;
    params.fReturnRemembered = 1;
    params.fReturnUnknown = 1;
    params.fReturnConnected = 1;
    params.fIssueInquiry = 1;
    params.cTimeoutMultiplier = timeout_units;
    params.hRadio = ptr::null_mut();

    let mut info: BLUETOOTH_DEVICE_INFO = unsafe { mem::zeroed() };
    info.dwSize = mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;

    // SAFETY: `params` is fully initialized; `info` has its dwSize set
    // (required by the API) and the rest is valid all-zeros for the
    // first call.
    let find = unsafe { BluetoothFindFirstDevice(&params, &mut info) };
    if find.is_null() {
        let err = unsafe { GetLastError() };
        if err == ERROR_NO_MORE_ITEMS {
            return Ok(Vec::new());
        }
        return Err(io::Error::from_raw_os_error(err as i32));
    }

    let mut found = Vec::new();
    loop {
        let device = device_from_info(&info);
        if is_wii_name(&device.name) {
            found.push(device);
        }
        // Reset for the next iteration; dwSize must be set again.
        info = unsafe { mem::zeroed() };
        info.dwSize = mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
        // SAFETY: `find` is the live handle from BluetoothFindFirstDevice;
        // `info` is reinitialized above.
        let ok = unsafe { BluetoothFindNextDevice(find, &mut info) };
        if ok == 0 {
            break;
        }
    }

    // SAFETY: `find` is the matching handle from FindFirstDevice.
    unsafe { BluetoothFindDeviceClose(find) };
    Ok(found)
}

/// Outcome of a [`pair_first`] call.
#[derive(Debug, Clone)]
pub struct PairResult {
    /// The device we paired with.
    pub address: [u8; 6],
    /// Its friendly name.
    pub name: String,
    /// `true` if the device was already paired and we just enabled
    /// HID service; `false` if a fresh pairing handshake happened.
    pub already_paired: bool,
}

/// Find the first Balance Board nearby, pair it (if not already
/// paired), and enable its HID service so it shows up as a normal
/// game controller in Windows.
pub fn pair_first(timeout: Duration) -> io::Result<PairResult> {
    let devices = scan(timeout)?;
    let board = devices
        .into_iter()
        .find(WiiDevice::is_balance_board)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "No Balance Board found. Press SYNC inside the battery cover and try again.",
            )
        })?;

    let radio = LocalRadio::open()?;
    eprintln!(
        "[pair] local Bluetooth radio MAC: {} (this is the PIN we'll send for SYNC pairing)",
        crate::pin::format_bd_addr(radio.address)
    );

    let mut info = info_for_address(board.address);
    let already_paired = info.fAuthenticated != 0;

    if !already_paired {
        authenticate(&radio, &mut info)?;
    }
    enable_hid_service(&radio, &info)?;

    Ok(PairResult {
        address: board.address,
        name: board.name,
        already_paired,
    })
}

/// RAII handle to the local Bluetooth radio. We need this for two
/// reasons:
///
/// 1. `BluetoothAuthenticateDeviceEx` and `BluetoothSendAuthenticationResponseEx`
///    work much more reliably with an explicit radio handle than with
///    `NULL` ("any radio") — passing NULL was producing
///    `ERROR_GEN_FAILURE` on Carter's setup.
/// 2. The Wii's SYNC-button pairing protocol uses the **host's**
///    Bluetooth radio MAC (in rgBytes / little-endian order) as the
///    PIN, not the device's own MAC. So we need to know our own
///    radio's address to derive the right PIN.
struct LocalRadio {
    handle: HANDLE,
    address: [u8; 6],
}

impl LocalRadio {
    fn open() -> io::Result<Self> {
        let mut find_params: BLUETOOTH_FIND_RADIO_PARAMS = unsafe { mem::zeroed() };
        find_params.dwSize = mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32;

        let mut handle: HANDLE = ptr::null_mut();
        // SAFETY: `find_params` is fully initialized; `handle` is an
        // out-parameter that will be set on success.
        let h_find = unsafe { BluetoothFindFirstRadio(&find_params, &mut handle) };
        if h_find.is_null() {
            let err = unsafe { GetLastError() };
            return Err(io::Error::other(format!(
                "BluetoothFindFirstRadio failed: os error {err}"
            )));
        }
        // We only need the first radio; close the find iterator now.
        // SAFETY: `h_find` is the live handle from FindFirstRadio.
        unsafe { BluetoothFindRadioClose(h_find) };

        let mut info: BLUETOOTH_RADIO_INFO = unsafe { mem::zeroed() };
        info.dwSize = mem::size_of::<BLUETOOTH_RADIO_INFO>() as u32;
        // SAFETY: `handle` is the live radio handle; `info` has its
        // dwSize set as required.
        let rc = unsafe { BluetoothGetRadioInfo(handle, &mut info) };
        if rc != ERROR_SUCCESS {
            // SAFETY: `handle` came from FindFirstRadio.
            unsafe { CloseHandle(handle) };
            return Err(io::Error::other(format!(
                "BluetoothGetRadioInfo failed: os error {rc}"
            )));
        }
        // SAFETY: `address.Anonymous.rgBytes` is the 6-byte alternative
        // view of a valid `BLUETOOTH_ADDRESS` union.
        let address = unsafe { info.address.Anonymous.rgBytes };
        Ok(LocalRadio { handle, address })
    }
}

impl Drop for LocalRadio {
    fn drop(&mut self) {
        // SAFETY: `handle` came from FindFirstRadio and isn't shared.
        unsafe { CloseHandle(self.handle) };
    }
}

/// Unpair every Balance Board currently known to Windows. Returns
/// the number removed.
pub fn forget_all_balance_boards() -> io::Result<usize> {
    let devices = scan(Duration::from_secs(2))?;
    let mut count = 0;
    for d in devices {
        if !d.is_balance_board() || !d.remembered {
            continue;
        }
        let mut addr: BLUETOOTH_ADDRESS = unsafe { mem::zeroed() };
        addr.Anonymous.rgBytes = d.address;
        // SAFETY: `addr` is fully initialized; `BluetoothRemoveDevice`
        // takes a pointer to a 6-byte address structure.
        let rc = unsafe { BluetoothRemoveDevice(&addr) };
        if rc == ERROR_SUCCESS {
            count += 1;
        }
    }
    Ok(count)
}

// --- Internals -----------------------------------------------------------

fn device_from_info(info: &BLUETOOTH_DEVICE_INFO) -> WiiDevice {
    // SAFETY: `Address.Anonymous.rgBytes` is the 6-byte alternative
    // view of a valid `BLUETOOTH_ADDRESS` union. Reading it as an
    // array of bytes is always defined.
    let address = unsafe { info.Address.Anonymous.rgBytes };
    WiiDevice {
        address,
        name: wide_to_string(&info.szName),
        authenticated: info.fAuthenticated != 0,
        connected: info.fConnected != 0,
        remembered: info.fRemembered != 0,
    }
}

fn wide_to_string(wide: &[u16]) -> String {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

fn info_for_address(address: [u8; 6]) -> BLUETOOTH_DEVICE_INFO {
    let mut info: BLUETOOTH_DEVICE_INFO = unsafe { mem::zeroed() };
    info.dwSize = mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
    info.Address.Anonymous.rgBytes = address;
    info
}

/// Context passed through Windows' auth callback so the callback can
/// build the response that includes our binary PIN.
struct AuthContext {
    pin: [u8; WII_PIN_LEN],
    /// Radio handle for `BluetoothSendAuthenticationResponseEx`.
    /// Must be the same radio we registered for auth on.
    radio_handle: HANDLE,
}

unsafe extern "system" fn auth_callback(
    pv_param: *const c_void,
    auth_params: *const BLUETOOTH_AUTHENTICATION_CALLBACK_PARAMS,
) -> i32 {
    if pv_param.is_null() || auth_params.is_null() {
        eprintln!("[auth_callback] null parameter, returning ERROR_INVALID_PARAMETER");
        return ERROR_INVALID_PARAMETER as i32;
    }
    // SAFETY: `pv_param` points at an `AuthContext` we registered and
    // own; lifetime extends until we call `BluetoothUnregisterAuthentication`.
    let ctx = unsafe { &*(pv_param.cast::<AuthContext>()) };
    // SAFETY: `auth_params` is provided by the OS; valid for the
    // duration of the callback.
    let params = unsafe { &*auth_params };

    eprintln!(
        "[auth_callback] fired. negotiated authMethod = {} (1=legacy, 2=oob, 3=numeric, 4=passkey-keyboard, 5=passkey-display)",
        params.authenticationMethod
    );

    let mut response: BLUETOOTH_AUTHENTICATE_RESPONSE = unsafe { mem::zeroed() };
    response.bthAddressRemote = params.deviceInfo.Address;
    response.authMethod = BLUETOOTH_AUTHENTICATION_METHOD_LEGACY;
    // SAFETY: zeroed `response` already has a valid `pinInfo` view of
    // its union; we fill in the legacy PIN fields.
    response.Anonymous.pinInfo.pin[..WII_PIN_LEN].copy_from_slice(&ctx.pin);
    response.Anonymous.pinInfo.pinLength = WII_PIN_LEN as u8;
    response.negativeResponse = 0;

    // SAFETY: `response` is fully initialized; `BluetoothSendAuthenticationResponseEx`
    // returns a Win32 error code (DWORD = u32).
    let rc = unsafe { BluetoothSendAuthenticationResponseEx(ctx.radio_handle, &response) };
    eprintln!(
        "[auth_callback] BluetoothSendAuthenticationResponseEx returned {} ({})",
        rc,
        if rc == 0 { "success" } else { "error" }
    );
    rc as i32
}

fn authenticate(radio: &LocalRadio, info: &mut BLUETOOTH_DEVICE_INFO) -> io::Result<()> {
    // The Wii's SYNC-button pairing PIN is the **host** PC's Bluetooth
    // radio MAC, not the device's own MAC. (For "1+2 button hold"
    // pairing on a Wiimote it'd be the wiimote's own MAC; the Balance
    // Board only does SYNC pairing.)
    let pin = wii_pin_for_address(radio.address);

    // Box and leak the context for the duration of registration; we
    // reclaim it after unregistering, below.
    let ctx = Box::new(AuthContext {
        pin,
        radio_handle: radio.handle,
    });
    let ctx_ptr = Box::into_raw(ctx);

    // windows-sys 0.61 models the registration handle as a bare `isize`
    // (the kernel-handle integer form), not a void pointer.
    let mut reg_handle: isize = 0;

    // SAFETY: `info` is initialized; `auth_callback` is a valid `extern
    // "system"` fn; `ctx_ptr` outlives the registration (we unregister
    // before dropping it).
    let rc = unsafe {
        BluetoothRegisterForAuthenticationEx(
            info,
            &mut reg_handle,
            Some(auth_callback),
            ctx_ptr.cast::<c_void>(),
        )
    };
    if rc != ERROR_SUCCESS {
        // SAFETY: Box::from_raw on a pointer we created via into_raw.
        unsafe { drop(Box::from_raw(ctx_ptr)) };
        return Err(io::Error::other(format!(
            "BluetoothRegisterForAuthenticationEx failed: os error {rc}"
        )));
    }

    eprintln!("[pair] auth callback registered, calling BluetoothAuthenticateDeviceEx...");

    // SAFETY: `info` is initialized.
    let auth_rc = unsafe {
        BluetoothAuthenticateDeviceEx(
            ptr::null_mut(), // hwndParent — none
            radio.handle,    // hRadio — explicit local radio
            info,
            ptr::null_mut(),                  // OOB data — none
            MITMProtectionNotRequiredBonding, // request persistent bonding (HID needs it)
        )
    };

    eprintln!("[pair] BluetoothAuthenticateDeviceEx returned {auth_rc}");

    // SAFETY: matching unregister for the registration above.
    unsafe { BluetoothUnregisterAuthentication(reg_handle) };
    // SAFETY: reclaim the box — callback can't fire after unregister.
    unsafe { drop(Box::from_raw(ctx_ptr)) };

    if auth_rc != ERROR_SUCCESS {
        return Err(io::Error::other(format!(
            "BluetoothAuthenticateDeviceEx failed: os error {auth_rc}"
        )));
    }
    Ok(())
}

fn enable_hid_service(radio: &LocalRadio, info: &BLUETOOTH_DEVICE_INFO) -> io::Result<()> {
    // GUID for the HID Service Class. From the Bluetooth SIG:
    // {0000_1124-0000-1000-8000-00805F9B34FB}
    let hid_guid = windows_sys::core::GUID {
        data1: 0x0000_1124,
        data2: 0x0000,
        data3: 0x1000,
        data4: [0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB],
    };
    // SAFETY: GUID is fully initialized; info is initialized; the
    // function takes them by-pointer for read.
    let rc = unsafe {
        BluetoothSetServiceState(radio.handle, info, &hid_guid, BLUETOOTH_SERVICE_ENABLE)
    };
    if rc != ERROR_SUCCESS {
        return Err(io::Error::other(format!(
            "BluetoothSetServiceState (HID enable) failed: os error {rc}"
        )));
    }
    eprintln!("[pair] HID service enabled.");
    Ok(())
}
