//! Windows Bluetooth glue: scan, pair, enable HID service, forget.
//!
//! This module is the only place that talks to `BluetoothAPIs.dll`.
//! Everything else in the crate (PIN math, CLI) is portable.
//!
//! SYNC pairing supplies the local radio address directly to the legacy
//! PIN API. This avoids depending on an authentication callback that may
//! never arrive for the board on Windows.

#![cfg(windows)]
#![allow(non_snake_case)]

use std::io;
use std::mem;
use std::ptr;
use std::time::Duration;

use windows_sys::Win32::Devices::Bluetooth::*;
use windows_sys::Win32::Foundation::*;

use crate::pin::{sync_passkey, WII_PIN_LEN};

/// One Wii-family device returned by [`scan`].
#[derive(Debug, Clone)]
pub struct WiiDevice {
    /// 6-byte Bluetooth address in Win32 `rgBytes` order (little-endian
    /// by host convention). SYNC pairing uses the local radio address instead.
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
    scan_on_radio(timeout, ptr::null_mut())
}

fn scan_on_radio(timeout: Duration, radio: HANDLE) -> io::Result<Vec<WiiDevice>> {
    let timeout_units = ((timeout.as_secs_f32() / 1.28).ceil() as u8).clamp(1, 48);

    let mut params: BLUETOOTH_DEVICE_SEARCH_PARAMS = unsafe { mem::zeroed() };
    params.dwSize = mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32;
    params.fReturnAuthenticated = 1;
    params.fReturnRemembered = 1;
    params.fReturnUnknown = 1;
    params.fReturnConnected = 1;
    params.fIssueInquiry = 1;
    params.cTimeoutMultiplier = timeout_units;
    params.hRadio = radio;

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
    let radio = LocalRadio::open()?;
    let devices = scan_on_radio(timeout, radio.handle)?;
    let board = devices
        .into_iter()
        .find(WiiDevice::is_balance_board)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "No Balance Board found. Press SYNC inside the battery cover and try again.",
            )
        })?;

    eprintln!(
        "[pair] local Bluetooth radio MAC: {}",
        crate::pin::format_bd_addr(radio.address)
    );

    let mut info = info_for_address(board.address);
    // Refresh the discovered device on the radio used for authentication.
    // A newly initialized address-only structure has fAuthenticated == 0.
    // https://learn.microsoft.com/en-us/windows/win32/api/bluetoothapis/nf-bluetoothapis-bluetoothgetdeviceinfo
    // SAFETY: radio is live; info has its required size and remote address.
    let rc = unsafe { BluetoothGetDeviceInfo(radio.handle, &mut info) };
    if rc != ERROR_SUCCESS {
        return Err(io::Error::other(format!(
            "BluetoothGetDeviceInfo failed: os error {rc}"
        )));
    }
    let already_paired = info.fAuthenticated != 0;

    if !already_paired {
        authenticate(&radio, &mut info)?;
    }
    discover_services(&radio, &info)?;
    enable_hid_service(&radio, &info)?;

    Ok(PairResult {
        address: board.address,
        name: board.name,
        already_paired,
    })
}

/// Own the same radio for discovery, PIN derivation, authentication, and HID
/// activation. Mixing an all-radio scan with a different radio's PIN can fail.
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

fn authenticate(radio: &LocalRadio, info: &mut BLUETOOTH_DEVICE_INFO) -> io::Result<()> {
    // Red SYNC pairing uses the host radio address in Bluetooth byte order.
    // Supply each byte in one WCHAR slot, with explicit length 6 (not hex text
    // and not pairs of bytes packed into u16s). Keep a trailing NUL for Win32.
    // References:
    // https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/HW/WiimoteReal/IOWin.cpp
    // https://learn.microsoft.com/en-us/windows/win32/api/bluetoothapis/nf-bluetoothapis-bluetoothauthenticatedevice
    let mut passkey = sync_passkey(radio.address);
    eprintln!("[pair] Authenticating with the local radio's SYNC PIN (direct legacy API)...");
    // SAFETY: radio is live; info has its size and address initialized.
    // passkey is a writable, terminated array, with six PIN code units.
    let rc = unsafe {
        BluetoothAuthenticateDevice(
            ptr::null_mut(),
            radio.handle,
            info,
            passkey.as_mut_ptr(),
            WII_PIN_LEN as u32,
        )
    };
    eprintln!("[pair] BluetoothAuthenticateDevice returned {rc}");
    if rc == ERROR_NO_MORE_ITEMS {
        // Another pairing attempt may have completed after discovery. Verify
        // Windows' current record before treating that result as success.
        // SAFETY: radio and info remain valid as above.
        let refresh = unsafe { BluetoothGetDeviceInfo(radio.handle, info) };
        if refresh == ERROR_SUCCESS && info.fAuthenticated != 0 {
            return Ok(());
        }
    }
    if rc != ERROR_SUCCESS {
        return Err(authentication_error(rc));
    }
    Ok(())
}

fn authentication_error(code: u32) -> io::Error {
    let (kind, advice) = match code {
        WAIT_TIMEOUT => (io::ErrorKind::TimedOut,
            "Windows timed out contacting/authenticating the board. This does not prove a bad PIN. Press red SYNC again immediately before retrying; keep the board nearby and close other Wii connection tools."),
        ERROR_NOT_AUTHENTICATED => (io::ErrorKind::PermissionDenied,
            "Windows reported authentication failure. This method expects red SYNC pairing; press red SYNC and retry."),
        ERROR_DEVICE_NOT_CONNECTED => (io::ErrorKind::NotConnected,
            "The board disconnected before pairing completed. Press red SYNC and retry."),
        _ => (io::ErrorKind::Other, "Pairing did not complete. Keep the full error for troubleshooting."),
    };
    io::Error::new(
        kind,
        format!(
            "{advice} (BluetoothAuthenticateDevice: {code}; {})",
            io::Error::from_raw_os_error(code as i32)
        ),
    )
}

fn discover_services(radio: &LocalRadio, info: &BLUETOOTH_DEVICE_INFO) -> io::Result<()> {
    let mut count = 0;
    // The service inquiry follows authentication before HID activation.
    // A null buffer requests the count; ERROR_MORE_DATA is a successful query.
    // SAFETY: valid radio/device and writable count; no GUID buffer is supplied.
    let rc = unsafe {
        BluetoothEnumerateInstalledServices(radio.handle, info, &mut count, ptr::null_mut())
    };
    if rc != ERROR_SUCCESS && rc != ERROR_MORE_DATA {
        return Err(io::Error::other(format!("Paired, but Bluetooth service discovery failed: os error {rc}. Wake the board and retry.")));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_is_distinguished_from_rejected_authentication() {
        let error = authentication_error(WAIT_TIMEOUT);
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("258"));
        assert!(error.to_string().contains("does not prove a bad PIN"));
        assert_eq!(
            authentication_error(ERROR_NOT_AUTHENTICATED).kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
