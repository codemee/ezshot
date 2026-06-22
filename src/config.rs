use std::path::{Path, PathBuf};

pub struct Config {
    pub save_dir: PathBuf,
    pub capture_cursor: bool,
    pub capture_delay_secs: u32,
    pub auto_copy: bool,
    pub hide_editor_on_capture: bool,
    pub language: String,
    pub theme: String,                // "auto" | "light" | "dark"
}

impl Default for Config {
    fn default() -> Self {
        let (cursor, delay, auto_copy, hide_on_capture, language, theme) = load_settings();
        Self {
            save_dir: load_save_dir(),
            capture_cursor: cursor,
            capture_delay_secs: delay,
            auto_copy,
            hide_editor_on_capture: hide_on_capture,
            language,
            theme,
        }
    }
}

// ── 儲存目錄（由編輯器執行緒寫入）──────────────────────────────────────

pub fn load_save_dir() -> PathBuf {
    config_dir()
        .map(|p| p.join("last_dir.txt"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.is_dir())
        .unwrap_or_else(default_dir)
}

pub fn persist_save_dir(dir: &Path) {
    if let Some(base) = config_dir() {
        let _ = std::fs::create_dir_all(&base);
        let _ = std::fs::write(base.join("last_dir.txt"), dir.to_string_lossy().as_bytes());
    }
}

// ── 擷取設定（由系統匣執行緒讀寫）──────────────────────────────────────

fn load_settings() -> (bool, u32, bool, bool, String, String) {
    let mut cursor          = false;
    let mut delay           = 0u32;
    let mut auto_copy       = false;
    let mut hide_on_capture = false;
    let mut language        = "auto".to_string();
    let mut theme           = "auto".to_string();
    let path = match config_dir().map(|p| p.join("settings.ini")) {
        Some(p) => p,
        None => return (cursor, delay, auto_copy, hide_on_capture, language, theme),
    };
    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines() {
            if let Some(v) = line.strip_prefix("capture_cursor=") {
                cursor = v == "1";
            } else if let Some(v) = line.strip_prefix("capture_delay_secs=") {
                delay = v.parse().unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("auto_copy=") {
                auto_copy = v == "1";
            } else if let Some(v) = line.strip_prefix("hide_editor_on_capture=") {
                hide_on_capture = v == "1";
            } else if let Some(v) = line.strip_prefix("language=") {
                language = v.to_string();
            } else if let Some(v) = line.strip_prefix("theme=") {
                theme = v.to_string();
            }
        }
    }
    (cursor, delay, auto_copy, hide_on_capture, language, theme)
}

pub fn persist_settings(config: &Config) {
    if let Some(base) = config_dir() {
        let _ = std::fs::create_dir_all(&base);
        let content = format!(
            "capture_cursor={}\ncapture_delay_secs={}\nauto_copy={}\nhide_editor_on_capture={}\nlanguage={}\ntheme={}\n",
            if config.capture_cursor { 1 } else { 0 },
            config.capture_delay_secs,
            if config.auto_copy { 1 } else { 0 },
            if config.hide_editor_on_capture { 1 } else { 0 },
            config.language,
            config.theme,
        );
        let _ = std::fs::write(base.join("settings.ini"), content.as_bytes());
    }
}

// ── 開機自動執行（HKCU Run 機碼）────────────────────────────────────────

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VAL: &str = "ezshot";

pub fn is_startup_with_windows() -> bool {
    unsafe {
        use windows::Win32::System::Registry::*;
        use windows::core::PCWSTR;
        let key: Vec<u16> = format!("{}\0", RUN_KEY).encode_utf16().collect();
        let val: Vec<u16> = format!("{}\0", RUN_VAL).encode_utf16().collect();
        let mut cb = 0u32;
        // RegGetValueW 若值不存在回傳錯誤；存在則 cb > 0
        let r = RegGetValueW(
            HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(val.as_ptr()),
            RRF_RT_REG_SZ, None, None, Some(&mut cb),
        );
        r.is_ok() || cb > 0
    }
}

pub fn set_startup_with_windows(enabled: bool) {
    unsafe {
        use windows::Win32::System::Registry::*;
        use windows::core::PCWSTR;
        let key: Vec<u16> = format!("{}\0", RUN_KEY).encode_utf16().collect();
        let val: Vec<u16> = format!("{}\0", RUN_VAL).encode_utf16().collect();
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()),
            0, KEY_SET_VALUE, &mut hkey).is_err() { return; }
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                let path_str = exe.to_string_lossy().to_string();
                let path_w: Vec<u16> = path_str.encode_utf16().chain(Some(0)).collect();
                let bytes = std::slice::from_raw_parts(
                    path_w.as_ptr() as *const u8, path_w.len() * 2);
                let _ = RegSetValueExW(hkey, PCWSTR(val.as_ptr()), 0, REG_SZ, Some(bytes));
            }
        } else {
            let _ = RegDeleteValueW(hkey, PCWSTR(val.as_ptr()));
        }
        let _ = RegCloseKey(hkey);
    }
}

// ── 共用輔助 ───────────────────────────────────────────────────────────

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("ezshot"))
}

fn default_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join("Desktop"))
        .unwrap_or_else(|| PathBuf::from("."))
}
