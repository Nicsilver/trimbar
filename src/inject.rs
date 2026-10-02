//! Browsers reassert their fullscreen rect (taken from GetMonitorInfo's rcMonitor) on every window
//! move, click and activation, so shrinking them from outside only flickers. Instead, trimbar loads
//! trimbar_hook.dll into browser UI threads; it reports trimmed monitors as shorter, and the browser
//! then sizes fullscreen correctly by itself. See hook/src/lib.rs.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowThreadProcessId, HHOOK, HOOKPROC, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_GETMESSAGE, WM_NULL,
};
use windows::core::{HSTRING, PWSTR, s, w};

use crate::fullscreen::Target;

const BROWSERS: &[&str] = &["chrome.exe", "msedge.exe", "brave.exe", "vivaldi.exe", "opera.exe", "firefox.exe"];

/// Must match `Shared` in hook/src/lib.rs.
#[repr(C)]
struct Shared {
    count: u32,
    entries: [Entry; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Entry {
    monitor: RECT,
    cut: i32,
}

struct State {
    proc: HOOKPROC,
    module: HMODULE,
    view: *mut Shared,
    hooks: HashMap<u32, HHOOK>,
    browsers: HashMap<u32, bool>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn init() {
    let Some(dll) = std::env::current_exe().ok().and_then(|p| Some(p.parent()?.join("trimbar_hook.dll"))) else {
        return;
    };
    unsafe {
        let Ok(module) = LoadLibraryW(&HSTRING::from(dll.as_os_str())) else { return };
        let Some(proc) = GetProcAddress(module, s!("trimbar_msg_hook")) else { return };
        let Ok(mapping) = CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            None,
            PAGE_READWRITE,
            0,
            size_of::<Shared>() as u32,
            w!(r"Local\Trimbar.Targets"),
        ) else {
            return;
        };
        // The mapping handle is intentionally never closed: it has to outlive every browser view.
        let _ = mapping;
        let view: MEMORY_MAPPED_VIEW_ADDRESS = MapViewOfFile(mapping, FILE_MAP_WRITE, 0, 0, size_of::<Shared>());
        if view.Value.is_null() {
            return;
        }
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                proc: Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT>(proc)),
                module,
                view: view.Value as *mut Shared,
                hooks: HashMap::new(),
                browsers: HashMap::new(),
            })
        });
    }
}

pub fn publish(targets: &[Target]) {
    STATE.with(|s| {
        let Some(state) = s.borrow().as_ref().map(|s| s.view) else { return };
        let mut shared = Shared { count: 0, entries: [Entry::default(); 16] };
        for (e, t) in shared.entries.iter_mut().zip(targets) {
            *e = Entry { monitor: t.monitor, cut: t.cut };
        }
        shared.count = targets.len().min(16) as u32;
        unsafe { std::ptr::write_volatile(state, shared) };
    });
}

pub fn shutdown() {
    publish(&[]);
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            for (_, h) in state.hooks.drain() {
                unsafe {
                    let _ = UnhookWindowsHookEx(h);
                }
            }
        }
    });
}

/// Makes sure the window's thread carries our hook if it belongs to a browser. Returns whether it is
/// a browser window, and whether the hook was installed just now.
pub fn ensure(hwnd: HWND) -> (bool, bool) {
    let mut pid = 0;
    let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if tid == 0 {
        return (false, false);
    }
    STATE.with(|s| {
        let mut guard = s.borrow_mut();
        let Some(state) = guard.as_mut() else { return (false, false) };
        let is_browser = *state.browsers.entry(pid).or_insert_with(|| is_browser(pid));
        if !is_browser {
            return (false, false);
        }
        let mut fresh = false;
        if !state.hooks.contains_key(&tid) {
            let hook = unsafe { SetWindowsHookExW(WH_GETMESSAGE, state.proc, Some(state.module.into()), tid) };
            if let Ok(hook) = hook {
                state.hooks.insert(tid, hook);
                fresh = true;
                // The DLL only loads once the thread pulls a message.
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                }
            }
        }
        (true, fresh)
    })
}

fn is_browser(pid: u32) -> bool {
    unsafe {
        let Ok(process): Result<HANDLE, _> = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]).to_ascii_lowercase();
        let name = path.rsplit('\\').next().unwrap_or("");
        BROWSERS.contains(&name)
    }
}

