#![windows_subsystem = "windows"]

mod config;
mod fullscreen;
mod inject;
mod monitors;
mod panel;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::{
    COLORREF, ERROR_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, EndPaint, HBRUSH, InvalidateRect, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VIRTUAL_KEY, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_UP,
};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABM_WINDOWPOSCHANGED,
    ABN_POSCHANGED, APPBARDATA, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFY_ICON_MESSAGE, NOTIFYICONDATAW, SHAppBarMessage, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const WM_TRAY: u32 = WM_APP + 1;
const WM_APPBAR: u32 = WM_APP + 2;
const WM_REPOSITION: u32 = WM_APP + 3;
const WM_OPEN_SETUP: u32 = WM_APP + 4;
const WM_FINISH_SETUP: u32 = WM_APP + 5;

const TIMER_REPOSITION: usize = 1;
const TIMER_REBUILD: usize = 2;

const CMD_ADJUST: i32 = 1;
const CMD_AUTOSTART: i32 = 2;
const CMD_EXIT: i32 = 3;
const CMD_FIT: i32 = 4;

const MAIN_CLASS: PCWSTR = w!("TrimbarMain");
const STRIP_CLASS: PCWSTR = w!("TrimbarStrip");
const OVERLAY_CLASS: PCWSTR = w!("TrimbarOverlay");
const PANEL_CLASS: PCWSTR = w!("TrimbarPanel");

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct Strip {
    hwnd: HWND,
    mon: usize,
    height: i32,
    rect: RECT,
}

struct Setup {
    draft: HashMap<String, i32>,
    panels: Vec<HWND>,
    overlays: Vec<HWND>,
}

struct App {
    hinst: HINSTANCE,
    main: HWND,
    icon: HICON,
    mons: Vec<monitors::Monitor>,
    saved: HashMap<String, i32>,
    strips: Vec<Strip>,
    setup: Option<Setup>,
}

// Win32 re-enters window procedures while we're inside our own calls (SHAppBarMessage pumps
// sent messages, CreateWindowEx sends WM_CREATE...). Window procs therefore only touch this
// from queued messages and use try_borrow, and anything triggered by a *sent* message is
// deferred through PostMessage or a timer.
thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

fn main_hwnd() -> HWND {
    HWND(MAIN_HWND.load(Ordering::Relaxed) as *mut _)
}

fn post_main(msg: u32, wparam: usize) {
    unsafe {
        let _ = PostMessageW(Some(main_hwnd()), msg, WPARAM(wparam), LPARAM(0));
    }
}

fn main() {
    unsafe {
        let _mutex = CreateMutexW(None, false, w!(r"Local\Trimbar.SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Launching it again is the obvious way to get the setup back, so hand off to
            // the running copy instead of silently exiting.
            if let Ok(existing) = FindWindowW(MAIN_CLASS, PCWSTR::null()) {
                let _ = PostMessageW(Some(existing), WM_OPEN_SETUP, WPARAM(0), LPARAM(0));
            }
            return;
        }

        let hinst: HINSTANCE = GetModuleHandleW(None).expect("module handle").into();
        register_class(hinst, MAIN_CLASS, main_proc, HBRUSH::default());
        register_class(hinst, STRIP_CLASS, strip_proc, CreateSolidBrush(COLORREF(0)));
        register_class(hinst, OVERLAY_CLASS, overlay_proc, CreateSolidBrush(COLORREF(panel::OVERLAY)));
        register_class(hinst, PANEL_CLASS, panel_proc, HBRUSH::default());

        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

        let main = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            MAIN_CLASS,
            w!("Trimbar"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )
        .expect("main window");
        MAIN_HWND.store(main.0 as isize, Ordering::Relaxed);

        let icon = LoadImageW(
            Some(hinst),
            PCWSTR(1 as _),
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        )
        .map(|h| HICON(h.0))
        .unwrap_or_default();

        let mut saved = config::load();
        let first_run = saved.is_none();
        if let Some(v) = saved.as_mut().and_then(|s| s.remove(config::FIT_KEY)) {
            fullscreen::ENABLED.store(v != 0, Ordering::Relaxed);
        }
        inject::init();
        fullscreen::install();
        if first_run || config::autostart_enabled() {
            // Rewriting it on every start keeps the Run entry valid if the exe was moved.
            config::set_autostart(true);
        }

        let app = App {
            hinst,
            main,
            icon,
            mons: monitors::enumerate(),
            saved: saved.unwrap_or_default(),
            strips: Vec::new(),
            setup: None,
        };
        app.tray(NIM_ADD);
        APP.with(|a| *a.borrow_mut() = Some(app));
        with_app(|a| a.apply_strips());
        if first_run {
            post_main(WM_OPEN_SETUP, 0);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

unsafe fn register_class(
    hinst: HINSTANCE,
    name: PCWSTR,
    proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    background: HBRUSH,
) {
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: hinst,
        lpszClassName: name,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        hbrBackground: background,
        ..Default::default()
    };
    unsafe { RegisterClassW(&wc) };
}

fn appbar_data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APPBAR,
        uEdge: ABE_BOTTOM,
        ..Default::default()
    }
}

/// Asks the shell where a `height`-tall bottom bar on `mon` may go. The shell may shrink the
/// rect around other appbars, so the height is re-applied from the adjusted bottom edge.
fn query_rect(hwnd: HWND, mon: RECT, height: i32) -> RECT {
    let mut abd = appbar_data(hwnd);
    abd.rc = RECT { top: mon.bottom - height, ..mon };
    unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut abd) };
    abd.rc.top = abd.rc.bottom - height;
    abd.rc
}

fn set_rect(hwnd: HWND, rect: RECT) -> RECT {
    let mut abd = appbar_data(hwnd);
    abd.rc = rect;
    unsafe {
        SHAppBarMessage(ABM_SETPOS, &mut abd);
        let r = abd.rc;
        let _ = SetWindowPos(
            hwnd,
            None,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
    abd.rc
}

impl App {
    fn tray(&self, op: NOTIFY_ICON_MESSAGE) -> bool {
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.main,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon,
            ..Default::default()
        };
        for (dst, src) in nid.szTip.iter_mut().zip("Trimbar".encode_utf16()) {
            *dst = src;
        }
        unsafe { Shell_NotifyIconW(op, &nid) }.as_bool()
    }

    fn height_for(&self, mon: usize) -> i32 {
        self.saved.get(&self.mons[mon].id).copied().unwrap_or(0)
    }

    fn apply_strips(&mut self) {
        self.remove_strips();
        for i in 0..self.mons.len() {
            let height = self.height_for(i);
            if height > 0 && let Some(strip) = self.create_strip(i, height) {
                self.strips.push(strip);
            }
        }
        let targets = self
            .strips
            .iter()
            .map(|s| fullscreen::Target { monitor: self.mons[s.mon].rect, cut: s.rect.top })
            .collect();
        fullscreen::set_targets(targets);
    }

    fn persist(&self) {
        let mut map = self.saved.clone();
        map.insert(config::FIT_KEY.to_string(), fullscreen::ENABLED.load(Ordering::Relaxed) as i32);
        let _ = config::save(&map);
    }

    fn create_strip(&self, mon: usize, height: i32) -> Option<Strip> {
        let m = self.mons[mon].rect;
        unsafe {
            // Not topmost: fullscreen apps and games should still cover the dead rows.
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                STRIP_CLASS,
                w!("Trimbar strip"),
                WS_POPUP,
                m.left,
                m.bottom - height,
                m.right - m.left,
                height,
                None,
                None,
                Some(self.hinst),
                None,
            )
            .ok()?;
            let mut abd = appbar_data(hwnd);
            if SHAppBarMessage(ABM_NEW, &mut abd) == 0 {
                let _ = DestroyWindow(hwnd);
                return None;
            }
            let rect = set_rect(hwnd, query_rect(hwnd, m, height));
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Some(Strip { hwnd, mon, height, rect })
        }
    }

    fn remove_strips(&mut self) {
        for s in self.strips.drain(..) {
            unsafe {
                let mut abd = appbar_data(s.hwnd);
                SHAppBarMessage(ABM_REMOVE, &mut abd);
                let _ = DestroyWindow(s.hwnd);
            }
        }
    }

    /// Another appbar (usually the taskbar) moved. Only re-set positions that actually change:
    /// our own ABM_SETPOS notifies the other strips, so blindly re-setting would ping-pong.
    fn reposition(&mut self) {
        for s in &mut self.strips {
            let target = query_rect(s.hwnd, self.mons[s.mon].rect, s.height);
            if target != s.rect {
                s.rect = set_rect(s.hwnd, target);
            }
        }
    }

    fn rebuild(&mut self) {
        let draft = self.setup.as_ref().map(|s| s.draft.clone());
        if draft.is_some() {
            self.close_setup();
        }
        if !self.tray(NIM_MODIFY) {
            self.tray(NIM_ADD);
        }
        self.mons = monitors::enumerate();
        self.apply_strips();
        if let Some(draft) = draft {
            self.open_setup(Some(draft));
        }
    }

    fn open_setup(&mut self, draft: Option<HashMap<String, i32>>) {
        if let Some(setup) = &self.setup {
            unsafe {
                let _ = SetForegroundWindow(setup.panels[0]);
            }
            return;
        }
        let mut draft = draft.unwrap_or_default();
        for i in 0..self.mons.len() {
            let h = self.height_for(i);
            draft.entry(self.mons[i].id.clone()).or_insert(h);
        }

        // Hidden appbars keep their reservation, so windows don't jump around while adjusting,
        // and the wallpaper (not a black strip) shows where the red bar isn't.
        for s in &self.strips {
            unsafe {
                let _ = ShowWindow(s.hwnd, SW_HIDE);
            }
        }

        let mut setup = Setup { draft, panels: Vec::new(), overlays: Vec::new() };
        for (i, m) in self.mons.iter().enumerate() {
            unsafe {
                let overlay = CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    OVERLAY_CLASS,
                    w!("Trimbar preview"),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(self.hinst),
                    None,
                )
                .unwrap_or_default();
                place_overlay(overlay, m.rect, setup.draft[&m.id]);

                let (mut dpi, mut _y) = (96u32, 96u32);
                let _ = GetDpiForMonitor(m.hmon, MDT_EFFECTIVE_DPI, &mut dpi, &mut _y);
                let (w, h) = (panel::WIDTH * dpi as i32 / 96, panel::HEIGHT * dpi as i32 / 96);
                let x = m.rect.left + (m.width() - w) / 2;
                let y = m.rect.top + (m.height() - h) * 2 / 5;
                let hwnd = CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    PANEL_CLASS,
                    w!("Trimbar"),
                    WS_POPUP,
                    x,
                    y,
                    w,
                    h,
                    None,
                    None,
                    Some(self.hinst),
                    None,
                )
                .unwrap_or_default();
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, i as isize);
                let _ = ShowWindow(hwnd, SW_SHOW);
                setup.panels.push(hwnd);
                setup.overlays.push(overlay);
            }
        }

        // Start on a side monitor: the main screen is rarely the broken one.
        let first = self.mons.iter().position(|m| !m.primary).unwrap_or(0);
        if let Some(&p) = setup.panels.get(first) {
            unsafe {
                let _ = SetForegroundWindow(p);
            }
        }
        self.setup = Some(setup);
    }

    fn adjust(&mut self, mon: usize, delta: i32) {
        let Some(setup) = &mut self.setup else { return };
        let m = &self.mons[mon];
        let v = setup.draft.entry(m.id.clone()).or_insert(0);
        *v = (*v + delta).clamp(0, m.height() / 3);
        place_overlay(setup.overlays[mon], m.rect, *v);
        unsafe {
            let _ = InvalidateRect(Some(setup.panels[mon]), None, false);
        }
    }

    fn close_setup(&mut self) -> Option<Setup> {
        let setup = self.setup.take()?;
        for &h in setup.panels.iter().chain(&setup.overlays) {
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        Some(setup)
    }

    fn finish_setup(&mut self, save: bool) {
        let Some(setup) = self.close_setup() else { return };
        if save {
            // Extend rather than replace: keeps trims for monitors that are unplugged right now.
            self.saved.extend(setup.draft);
            self.persist();
            self.apply_strips();
        } else {
            for s in &self.strips {
                unsafe {
                    let _ = ShowWindow(s.hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
    }

    fn shutdown(&mut self) {
        inject::shutdown();
        self.close_setup();
        self.remove_strips();
        self.tray(NIM_DELETE);
    }
}

fn place_overlay(hwnd: HWND, mon: RECT, height: i32) {
    unsafe {
        if height <= 0 {
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                mon.left,
                mon.bottom - height,
                mon.right - mon.left,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let autostart = config::autostart_enabled();
        let _ = AppendMenuW(menu, MF_STRING, CMD_ADJUST as usize, w!("Adjust trim…"));
        let _ = AppendMenuW(
            menu,
            MF_STRING | if autostart { MF_CHECKED } else { MF_UNCHECKED },
            CMD_AUTOSTART as usize,
            w!("Start with Windows"),
        );
        let fit = fullscreen::ENABLED.load(Ordering::Relaxed);
        let _ = AppendMenuW(
            menu,
            MF_STRING | if fit { MF_CHECKED } else { MF_UNCHECKED },
            CMD_FIT as usize,
            w!("Keep fullscreen apps above the trim"),
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, w!("Exit"));
        let _ = SetMenuDefaultItem(menu, CMD_ADJUST as u32, 0);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Without this the menu won't close when clicking elsewhere (documented TrackPopupMenu quirk).
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        match cmd.0 {
            CMD_ADJUST => post_main(WM_OPEN_SETUP, 0),
            CMD_AUTOSTART => config::set_autostart(!autostart),
            CMD_FIT => {
                fullscreen::ENABLED.store(!fit, Ordering::Relaxed);
                with_app(|a| a.persist());
                fullscreen::fit_all();
            }
            CMD_EXIT => {
                with_app(|a| a.shutdown());
                PostQuitMessage(0);
            }
            _ => {}
        }
    }
}

/// Runs `f` now, or retries shortly if the app state is busy (we're nested inside our own call).
fn run_or_retry(hwnd: HWND, timer: usize, f: impl FnOnce(&mut App)) {
    if with_app(f).is_none() {
        unsafe { SetTimer(Some(hwnd), timer, 100, None) };
    }
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TRAY => match lp.0 as u32 {
                WM_LBUTTONUP => post_main(WM_OPEN_SETUP, 0),
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            },
            WM_OPEN_SETUP => {
                with_app(|a| a.open_setup(None));
            }
            WM_FINISH_SETUP => {
                with_app(|a| a.finish_setup(wp.0 != 0));
            }
            WM_REPOSITION => {
                SetTimer(Some(hwnd), TIMER_REPOSITION, 150, None);
            }
            // Monitors were added, removed, moved or changed resolution. Wait for the layout to
            // settle; Windows sends several of these in a row.
            WM_DISPLAYCHANGE => {
                SetTimer(Some(hwnd), TIMER_REBUILD, 1000, None);
            }
            WM_TIMER => {
                let _ = KillTimer(Some(hwnd), wp.0);
                match wp.0 {
                    TIMER_REPOSITION => run_or_retry(hwnd, TIMER_REPOSITION, App::reposition),
                    TIMER_REBUILD => run_or_retry(hwnd, TIMER_REBUILD, App::rebuild),
                    _ => {}
                }
            }
            WM_ENDSESSION if wp.0 != 0 => {
                with_app(|a| a.shutdown());
            }
            // `taskkill /PID` without /F lands here; exit cleanly so the shell drops our appbars.
            WM_CLOSE => {
                with_app(|a| a.shutdown());
                PostQuitMessage(0);
            }
            // Explorer restarted: the tray icon and every appbar registration are gone.
            m if m == TASKBAR_CREATED.load(Ordering::Relaxed) && m != 0 => {
                SetTimer(Some(hwnd), TIMER_REBUILD, 1000, None);
            }
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
        LRESULT(0)
    }
}

unsafe extern "system" fn strip_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_APPBAR => {
                if wp.0 as u32 == ABN_POSCHANGED {
                    post_main(WM_REPOSITION, 0);
                }
                LRESULT(0)
            }
            // Appbar etiquette from the SHAppBarMessage docs; keeps the shell's z-order logic happy.
            WM_ACTIVATE => {
                let mut abd = appbar_data(hwnd);
                SHAppBarMessage(ABM_ACTIVATE, &mut abd);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_WINDOWPOSCHANGED => {
                let mut abd = appbar_data(hwnd);
                SHAppBarMessage(ABM_WINDOWPOSCHANGED, &mut abd);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn panel_mon(hwnd: HWND) -> usize {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize }
}

fn step() -> i32 {
    if unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0 { 10 } else { 1 }
}

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                APP.with(|a| {
                    let Ok(guard) = a.try_borrow() else { return };
                    let Some(app) = guard.as_ref() else { return };
                    let Some(setup) = &app.setup else { return };
                    let Some(m) = app.mons.get(panel_mon(hwnd)) else { return };
                    let title = panel::title(&m.label, m.width(), m.height(), m.primary);
                    panel::paint(
                        hdc,
                        &panel::View {
                            title: &title,
                            height: setup.draft.get(&m.id).copied().unwrap_or(0),
                            active: GetForegroundWindow() == hwnd,
                            dpi: GetDpiForWindow(hwnd),
                        },
                    );
                });
                let _ = EndPaint(hwnd, &ps);
            }
            WM_ACTIVATE => {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            WM_KEYDOWN => {
                let mon = panel_mon(hwnd);
                let delta = match VIRTUAL_KEY(wp.0 as u16) {
                    VK_UP | VK_RIGHT => step(),
                    VK_DOWN | VK_LEFT => -step(),
                    VK_PRIOR => 10,
                    VK_NEXT => -10,
                    VK_RETURN => {
                        post_main(WM_FINISH_SETUP, 1);
                        0
                    }
                    VK_ESCAPE => {
                        post_main(WM_FINISH_SETUP, 0);
                        0
                    }
                    _ => 0,
                };
                if delta != 0 {
                    with_app(|a| a.adjust(mon, delta));
                }
            }
            WM_MOUSEWHEEL => {
                let notches = (wp.0 >> 16) as u16 as i16;
                let delta = if notches > 0 { step() } else { -step() };
                with_app(|a| a.adjust(panel_mon(hwnd), delta));
            }
            WM_LBUTTONDOWN => {
                let (x, y) = (lp.0 as i16 as i32, (lp.0 >> 16) as i16 as i32);
                let mon = panel_mon(hwnd);
                match panel::hit_test(x, y, GetDpiForWindow(hwnd)) {
                    Some(panel::Button::Minus10) => _ = with_app(|a| a.adjust(mon, -10)),
                    Some(panel::Button::Minus1) => _ = with_app(|a| a.adjust(mon, -1)),
                    Some(panel::Button::Plus1) => _ = with_app(|a| a.adjust(mon, 1)),
                    Some(panel::Button::Plus10) => _ = with_app(|a| a.adjust(mon, 10)),
                    Some(panel::Button::Cancel) => post_main(WM_FINISH_SETUP, 0),
                    Some(panel::Button::Save) => post_main(WM_FINISH_SETUP, 1),
                    None => {}
                }
            }
            WM_CLOSE => post_main(WM_FINISH_SETUP, 0),
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
        LRESULT(0)
    }
}
