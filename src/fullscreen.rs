//! Fullscreen windows (browser video, games, players) ignore the work area, so the appbar alone
//! can't keep them out of the dead rows. This watches for windows that exactly cover a trimmed
//! monitor and shrinks them to end above the strip.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_FOREGROUND, EnumWindows, GA_ROOT, GetAncestor, GetClassNameW,
    GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SWP_ASYNCWINDOWPOS,
    SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};
use windows::core::BOOL;

const OBJID_WINDOW: i32 = 0;
const CHILDID_SELF: i32 = 0;

/// An app that snaps back to full size this often within FIGHT_WINDOW is left alone, so we never
/// get into a resize war with exclusive-fullscreen games.
const FIGHT_LIMIT: u32 = 5;
const FIGHT_WINDOW: Duration = Duration::from_secs(10);

pub static ENABLED: AtomicBool = AtomicBool::new(true);

pub struct Target {
    pub monitor: RECT,
    /// First dead row; fitted windows end here.
    pub cut: i32,
}

thread_local! {
    static TARGETS: RefCell<Vec<Target>> = const { RefCell::new(Vec::new()) };
    static FIGHTS: RefCell<HashMap<isize, (Instant, u32)>> = RefCell::new(HashMap::new());
    static GAVE_UP: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
}

pub fn install() {
    unsafe {
        for event in [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_LOCATIONCHANGE] {
            SetWinEventHook(event, event, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
        }
    }
}

pub fn set_targets(targets: Vec<Target>) {
    TARGETS.with(|t| *t.borrow_mut() = targets);
    fit_all();
}

pub fn fit_all() {
    unsafe {
        let _ = EnumWindows(Some(on_enum), LPARAM(0));
    }
}

unsafe extern "system" fn on_enum(hwnd: HWND, _: LPARAM) -> BOOL {
    fit(hwnd);
    TRUE
}

unsafe extern "system" fn on_event(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _: u32,
    _: u32,
) {
    // LOCATIONCHANGE also fires for carets and the cursor; only whole windows matter.
    if id_object == OBJID_WINDOW && id_child == CHILDID_SELF {
        fit(hwnd);
    }
}

fn fit(hwnd: HWND) {
    if !ENABLED.load(Ordering::Relaxed) || hwnd.is_invalid() {
        return;
    }
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || GetAncestor(hwnd, GA_ROOT) != hwnd {
            return;
        }
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return;
        }
        // Exact match only: a maximized window overhangs every edge by its border, and with a
        // small trim a loose match would catch it too.
        let Some((m, cut)) =
            TARGETS.with(|t| t.borrow().iter().find(|t| t.monitor == r).map(|t| (t.monitor, t.cut)))
        else {
            // Left fullscreen (and isn't just sitting in our fitted rect): forgive it, so a browser
            // that once fought back still gets fitted the next time it goes fullscreen.
            let fitted = TARGETS.with(|t| t.borrow().iter().any(|t| r == RECT { bottom: t.cut, ..t.monitor }));
            if !fitted {
                forgive(hwnd);
            }
            return;
        };
        if is_shell_or_own(hwnd) || !note_attempt(hwnd) {
            return;
        }
        // Async: a hung app must not block our message loop.
        let _ = SetWindowPos(
            hwnd,
            None,
            m.left,
            m.top,
            m.right - m.left,
            cut - m.top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
        );
    }
}

fn is_shell_or_own(hwnd: HWND) -> bool {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return true;
        }
        let mut buf = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut buf) as usize;
        let class = String::from_utf16_lossy(&buf[..len]);
        matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
    }
}

fn forgive(hwnd: HWND) {
    let key = hwnd.0 as isize;
    GAVE_UP.with(|g| g.borrow_mut().remove(&key));
    FIGHTS.with(|f| f.borrow_mut().remove(&key));
}

/// Returns false once a window has fought back too often.
fn note_attempt(hwnd: HWND) -> bool {
    let key = hwnd.0 as isize;
    if GAVE_UP.with(|g| g.borrow().contains(&key)) {
        return false;
    }
    let now = Instant::now();
    let give_up = FIGHTS.with(|f| {
        let mut f = f.borrow_mut();
        if f.len() > 256 {
            f.retain(|&k, _| unsafe { IsWindow(Some(HWND(k as *mut _))) }.as_bool());
        }
        let entry = f.entry(key).or_insert((now, 0));
        if now.duration_since(entry.0) > FIGHT_WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 > FIGHT_LIMIT
    });
    if give_up {
        GAVE_UP.with(|g| {
            let mut g = g.borrow_mut();
            if g.len() > 256 {
                g.retain(|&k| unsafe { IsWindow(Some(HWND(k as *mut _))) }.as_bool());
            }
            g.insert(key);
        });
    }
    !give_up
}
