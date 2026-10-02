//! Loaded into browsers by trimbar via SetWindowsHookEx. Browsers size fullscreen windows from
//! GetMonitorInfo's rcMonitor and force that rect on every window move, so resizing them from the
//! outside turns into a fight. Reporting a shorter rcMonitor for trimmed monitors makes the browser
//! itself lay out fullscreen above the dead rows.

use std::ffi::c_void;
use std::sync::Once;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleHandleExW, GetModuleHandleW,
    GetProcAddress,
};
use windows::Win32::System::Memory::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW};
use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, HHOOK};
use windows::core::{BOOL, PCWSTR, s, w};

/// Must match `fullscreen::Shared` in the trimbar exe.
#[repr(C)]
pub struct Shared {
    pub count: u32,
    pub entries: [Entry; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub monitor: RECT,
    pub cut: i32,
}

type GetMonitorInfoWFn = unsafe extern "system" fn(HMONITOR, *mut MONITORINFO) -> BOOL;

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static SHARED: AtomicPtr<Shared> = AtomicPtr::new(std::ptr::null_mut());
static INSTALL: Once = Once::new();

/// The hook procedure trimbar registers. Its only job is getting this DLL loaded; the real work
/// happens once, outside the loader lock (MinHook suspends threads, which DllMain must not do).
#[unsafe(no_mangle)]
pub extern "system" fn trimbar_msg_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    INSTALL.call_once(install);
    unsafe { CallNextHookEx(Some(HHOOK::default()), code, wparam, lparam) }
}

fn install() {
    unsafe {
        // Pinned: trimbar exiting removes its hook, which would otherwise unload us while the
        // browser might be inside our detour.
        let mut module = Default::default();
        let _ = GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
            PCWSTR(install as *const u16),
            &mut module,
        );

        if let Ok(mapping) = OpenFileMappingW(FILE_MAP_READ.0, false, w!(r"Local\Trimbar.Targets")) {
            let view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, size_of::<Shared>());
            SHARED.store(view.Value as *mut Shared, Ordering::Release);
        }

        let Ok(user32) = GetModuleHandleW(w!("user32.dll")) else { return };
        let Some(target) = GetProcAddress(user32, s!("GetMonitorInfoW")) else { return };
        let target = target as *mut c_void;
        if let Ok(original) = minhook::MinHook::create_hook(target, get_monitor_info_w as *mut c_void) {
            ORIGINAL.store(original as usize, Ordering::Release);
            let _ = minhook::MinHook::enable_hook(target);
        }
    }
}

unsafe extern "system" fn get_monitor_info_w(hmon: HMONITOR, info: *mut MONITORINFO) -> BOOL {
    let original: GetMonitorInfoWFn = unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
    let ok = unsafe { original(hmon, info) };
    if ok.as_bool() && !info.is_null() {
        unsafe { trim(&mut *info) };
    }
    ok
}

unsafe fn trim(info: &mut MONITORINFO) {
    let shared = SHARED.load(Ordering::Acquire);
    if shared.is_null() {
        return;
    }
    // Read through a volatile copy: trimbar rewrites this block whenever the trim changes.
    let data = unsafe { std::ptr::read_volatile(shared) };
    for e in data.entries.iter().take((data.count as usize).min(16)) {
        if e.monitor == info.rcMonitor {
            info.rcMonitor.bottom = e.cut;
            info.rcWork.bottom = info.rcWork.bottom.min(e.cut);
            return;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(_: HINSTANCE, _: u32, _: *mut c_void) -> BOOL {
    TRUE
}
