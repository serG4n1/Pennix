//! Настройки, горячая клавиша и файл «аварийного восстановления» для Pennix.

use crate::x11::HiddenWin;
use eframe::egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Hotkey {
    pub keysym: u32,
    pub key_name: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub sup: bool,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self { keysym: 0x71, key_name: "Q".into(), ctrl: true, shift: false, alt: true, sup: false }
    }
}

impl Hotkey {
    /// Маска модификаторов X11.
    pub fn x_mods(&self) -> u16 {
        (self.shift as u16) | (self.ctrl as u16) << 2 | (self.alt as u16) << 3 | (self.sup as u16) << 6
    }

    pub fn parts(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.ctrl { v.push("Ctrl".into()) }
        if self.alt { v.push("Alt".into()) }
        if self.shift { v.push("Shift".into()) }
        if self.sup { v.push("Super".into()) }
        v.push(self.key_name.clone());
        v
    }

    pub fn label(&self) -> String {
        self.parts().join(" + ")
    }

    pub fn from_egui(key: Key, m: Modifiers) -> Option<Self> {
        let (keysym, name) = key_to_keysym(key)?;
        Some(Self { keysym, key_name: name, ctrl: m.ctrl, shift: m.shift, alt: m.alt, sup: m.mac_cmd })
    }
}

fn key_to_keysym(key: Key) -> Option<(u32, String)> {
    use Key::*;
    let name = key.name().to_string();
    let ks: u32 = match key {
        A | B | C | D | E | F | G | H | I | J | K | L | M | N | O | P | Q | R | S | T | U | V | W | X
        | Y | Z => 0x61 + (name.as_bytes()[0] - b'A') as u32,
        Num0 | Num1 | Num2 | Num3 | Num4 | Num5 | Num6 | Num7 | Num8 | Num9 => {
            0x30 + (name.as_bytes()[0] - b'0') as u32
        }
        F1 => 0xffbe, F2 => 0xffbf, F3 => 0xffc0, F4 => 0xffc1, F5 => 0xffc2, F6 => 0xffc3,
        F7 => 0xffc4, F8 => 0xffc5, F9 => 0xffc6, F10 => 0xffc7, F11 => 0xffc8, F12 => 0xffc9,
        Escape => 0xff1b, Tab => 0xff09, Backspace => 0xff08, Enter => 0xff0d, Space => 0x20,
        Insert => 0xff63, Delete => 0xffff, Home => 0xff50, End => 0xff57,
        PageUp => 0xff55, PageDown => 0xff56,
        ArrowLeft => 0xff51, ArrowUp => 0xff52, ArrowRight => 0xff53, ArrowDown => 0xff54,
        Minus => 0x2d, Equals | Plus => 0x3d, Comma => 0x2c, Period => 0x2e, Slash => 0x2f,
        Backslash => 0x5c, Semicolon => 0x3b, Quote => 0x27, Backtick => 0x60,
        OpenBracket => 0x5b, CloseBracket => 0x5d,
        _ => return None,
    };
    let pretty = match key {
        ArrowLeft => "←".into(), ArrowUp => "↑".into(), ArrowRight => "→".into(), ArrowDown => "↓".into(),
        _ => name,
    };
    Some((ks, pretty))
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionMode {
    #[default]
    Apps,
    Windows,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowTarget {
    #[serde(default)]
    pub id: u32,
    pub title: String,
    pub class: String,
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub hotkey: Hotkey,
    #[serde(default)]
    pub mode: SelectionMode,
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub windows: Vec<WindowTarget>,
    #[serde(default = "default_true")]
    pub hide_self: bool,
    #[serde(default = "default_true")]
    pub hide_tray: bool,
    #[serde(default = "default_true")]
    pub mute_audio: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: Hotkey::default(),
            mode: SelectionMode::Apps,
            classes: Vec::new(),
            windows: Vec::new(),
            hide_self: true,
            hide_tray: true,
            mute_audio: true,
        }
    }
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("pennix")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(config_dir().join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = config_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join("config.json"), s);
        }
    }
}

/// Список спрятанных окон на диске.
pub fn save_hidden(list: &[HiddenWin]) {
    let path = config_dir().join("hidden.json");
    if list.is_empty() {
        let _ = std::fs::remove_file(path);
    } else {
        let _ = std::fs::create_dir_all(config_dir());
        if let Ok(s) = serde_json::to_string(list) {
            let _ = std::fs::write(path, s);
        }
    }
}

pub fn load_hidden() -> Vec<HiddenWin> {
    std::fs::read_to_string(config_dir().join("hidden.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Список спрятанных элементов трея на диске.
pub fn save_hidden_tray(keys: &[String]) {
    let path = config_dir().join("hidden_tray.json");
    if keys.is_empty() {
        let _ = std::fs::remove_file(path);
    } else {
        let _ = std::fs::create_dir_all(config_dir());
        if let Ok(s) = serde_json::to_string(keys) {
            let _ = std::fs::write(path, s);
        }
    }
}

pub fn load_hidden_tray() -> Vec<String> {
    std::fs::read_to_string(config_dir().join("hidden_tray.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
