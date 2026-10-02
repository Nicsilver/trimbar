use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_VALUE: PCWSTR = w!("Trimbar");

fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Trimbar").join("config.txt")
}

/// `None` means trimbar has never been set up on this machine.
pub fn load() -> Option<HashMap<String, i32>> {
    let text = fs::read_to_string(path()).ok()?;
    let map = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        // rsplit: the key is a device path, the number is always last.
        .filter_map(|l| l.rsplit_once('='))
        .filter_map(|(k, v)| Some((k.trim().to_string(), v.trim().parse().ok()?)))
        .collect();
    Some(map)
}

pub fn save(map: &HashMap<String, i32>) -> io::Result<()> {
    let path = path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut keys: Vec<_> = map.keys().collect();
    keys.sort();
    let mut text = String::from("# Trimbar: pixel rows hidden at the bottom of each monitor\n");
    for k in keys {
        text.push_str(&format!("{k}={}\n", map[k]));
    }
    fs::write(path, text)
}

pub fn autostart_enabled() -> bool {
    let err = unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, None) };
    err == ERROR_SUCCESS
}

pub fn set_autostart(on: bool) {
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let cmd: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain([0]).collect();
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            REG_SZ.0,
            Some(cmd.as_ptr().cast()),
            (cmd.len() * 2) as u32,
        );
    }
}
