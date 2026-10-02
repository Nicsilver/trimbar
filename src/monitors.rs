use windows::Win32::Foundation::{LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
    MONITORINFO, MONITORINFOEXW,
};
use windows::core::{BOOL, PCWSTR};

const EDD_GET_DEVICE_INTERFACE_NAME: u32 = 1;
const MONITORINFOF_PRIMARY: u32 = 1;

pub struct Monitor {
    /// Device interface path of the attached monitor. Unlike `\\.\DISPLAYn` it survives
    /// renumbering, and two identical monitors on different ports still get different ids.
    pub id: String,
    pub label: String,
    pub rect: RECT,
    pub hmon: HMONITOR,
    pub primary: bool,
}

impl Monitor {
    pub fn width(&self) -> i32 {
        self.rect.right - self.rect.left
    }

    pub fn height(&self) -> i32 {
        self.rect.bottom - self.rect.top
    }
}

pub fn enumerate() -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(on_monitor), LPARAM(&mut out as *mut _ as isize));
    }
    out.sort_by_key(|m| (m.rect.left, m.rect.top));
    out
}

unsafe extern "system" fn on_monitor(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let out = unsafe { &mut *(data.0 as *mut Vec<Monitor>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if !unsafe { GetMonitorInfoW(hmon, &mut info as *mut _ as *mut MONITORINFO) }.as_bool() {
        return TRUE;
    }

    let device = from_wide(&info.szDevice);
    let mut dd = DISPLAY_DEVICEW { cb: size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
    let found = unsafe {
        EnumDisplayDevicesW(PCWSTR(info.szDevice.as_ptr()), 0, &mut dd, EDD_GET_DEVICE_INTERFACE_NAME)
    };
    let interface = if found.as_bool() { from_wide(&dd.DeviceID) } else { String::new() };

    let number = device.trim_start_matches(r"\\.\DISPLAY");
    out.push(Monitor {
        id: if interface.is_empty() { device.clone() } else { interface },
        label: format!("Display {number}"),
        rect: info.monitorInfo.rcMonitor,
        hmon,
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    });
    TRUE
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}
