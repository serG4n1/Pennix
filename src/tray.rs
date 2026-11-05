use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrayItem {
    pub service: String,
    pub path: String,
    pub id: String,
    pub title: String,
    pub icon_name: String,
    pub icon_file: Option<PathBuf>,
    pub pid: Option<u32>,
    pub class: String,
}

fn eval_cinnamon(js: &str) -> Result<String, String> {
    let out = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.Cinnamon",
            "--object-path",
            "/org/Cinnamon",
            "--method",
            "org.Cinnamon.Eval",
            js,
        ])
        .output()
        .map_err(|e| format!("gdbus error: {e}"))?;

    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.starts_with("(true, ") {
        let inner = s.trim_start_matches("(true, ").trim_end_matches(')').trim();
        let trimmed = if inner.starts_with('\'') && inner.ends_with('\'') {
            &inner[1..inner.len() - 1]
        } else {
            inner
        };
        // Раскодируем экранированные кавычки Cinnamon JSON
        let unescaped = trimmed.replace("\\\"", "\"").replace("\\\\", "\\");
        Ok(unescaped)
    } else {
        Err(s)
    }
}

/// Получает список зарегистрированных элементов из StatusNotifierWatcher.
fn registered_sni_items() -> Vec<String> {
    for dest in ["org.kde.StatusNotifierWatcher", "org.x.StatusNotifierWatcher"] {
        let out = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                dest,
                "--object-path",
                "/StatusNotifierWatcher",
                "--method",
                "org.freedesktop.DBus.Properties.Get",
                dest,
                "RegisteredStatusNotifierItems",
            ])
            .output();

        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.contains('[') {
                let mut items = Vec::new();
                for part in s.split('\'') {
                    if part.contains('/') && (part.starts_with(':') || part.starts_with("org.")) {
                        items.push(part.to_string());
                    }
                }
                if !items.is_empty() {
                    return items;
                }
            }
        }
    }
    Vec::new()
}

/// Получает PID процесса по его уникальному D-Bus имени (:1.xx).
fn get_dbus_pid(bus_name: &str) -> Option<u32> {
    let out = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.GetConnectionUnixProcessID",
            bus_name,
        ])
        .output()
        .ok()?;

    let s = String::from_utf8_lossy(&out.stdout);
    for word in s.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_ascii_digit());
        if let Ok(pid) = clean.parse::<u32>() {
            return Some(pid);
        }
    }
    None
}

/// Имя процесса из /proc/<pid>/comm.
fn get_proc_comm(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
}

/// Ищет файл иконки на диске.
fn find_icon_file(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let p = Path::new(name);
    if p.is_file() {
        return Some(p.to_path_buf());
    }

    // Поиск в стандартных директориях иконок
    let bases = [
        "/usr/share/pixmaps",
        "/usr/share/icons/hicolor/256x256/apps",
        "/usr/share/icons/hicolor/128x128/apps",
        "/usr/share/icons/hicolor/48x48/apps",
        "/usr/share/icons/hicolor/scalable/apps",
        "/usr/share/icons/Mint-Y/apps/48",
        "/usr/share/icons/Mint-X/apps/48",
        "/var/lib/flatpak/exports/share/icons/hicolor/256x256/apps",
        "/var/lib/flatpak/exports/share/icons/hicolor/128x128/apps",
        "/var/lib/flatpak/exports/share/icons/hicolor/48x48/apps",
    ];

    for base in bases {
        for ext in [".png", ".svg", ".xpm"] {
            let candidate = Path::new(base).join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Опрашивает все доступные иконки в трее.
pub fn list_tray_items() -> Vec<TrayItem> {
    let raw_items = registered_sni_items();
    let mut results = Vec::new();

    for item in raw_items {
        // Формат: :1.83/org/ayatana/NotificationItem/...
        let slash = match item.find('/') {
            Some(i) => i,
            None => continue,
        };
        let service = &item[..slash];
        let path = &item[slash..];

        let pid = get_dbus_pid(service);
        let comm = pid.and_then(get_proc_comm);

        // Запрашиваем свойства через GetAll
        let out = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                service,
                "--object-path",
                path,
                "--method",
                "org.freedesktop.DBus.Properties.GetAll",
                "org.kde.StatusNotifierItem",
            ])
            .output();

        let mut id = String::new();
        let mut title = String::new();
        let mut icon_name = String::new();

        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            if let Some(pos) = s.find("'Id': <'") {
                id = s[pos + 8..].split('\'').next().unwrap_or("").to_string();
            }
            if let Some(pos) = s.find("'Title': <'") {
                title = s[pos + 11..].split('\'').next().unwrap_or("").to_string();
            }
            if let Some(pos) = s.find("'IconName': <'") {
                icon_name = s[pos + 14..].split('\'').next().unwrap_or("").to_string();
            }
        }

        // Определяем нормализованный класс (для сопоставления с окнами и для выбора пользователем)
        let mut cand = Vec::new();
        if let Some(c) = comm.as_ref() {
            if c != "xdg-dbus-proxy" && c != "bwrap" && c != "flatpak-spawn" && c != "systemd" && !c.is_empty() {
                cand.push(c.clone());
            }
        }
        if !title.is_empty() {
            cand.push(title.clone());
        }
        if !id.is_empty() {
            if id.contains(' ') {
                if let Some(last) = id.split_whitespace().last() {
                    cand.push(last.to_string());
                }
            }
            cand.push(id.clone());
        }

        let class = cand.into_iter().next().unwrap_or_else(|| service.to_string());
        let icon_file = find_icon_file(&icon_name);

        results.push(TrayItem {
            service: service.to_string(),
            path: path.to_string(),
            id,
            title,
            icon_name,
            icon_file,
            pid,
            class,
        });
    }

    results
}

/// Скрывает иконки выбранных приложений в системном трее Cinnamon.
pub fn hide_tray_items(targets: &[String]) -> Vec<String> {
    if targets.is_empty() {
        return Vec::new();
    }

    let targets_json = serde_json::to_string(targets).unwrap_or_else(|_| "[]".into());

    let script = format!(
        r#"
(() => {{
    try {{
        let targets = {targets_json};
        function getTokens(str) {{
            if (!str) return [];
            let lower = str.toLowerCase();
            let words = lower.split(/[^a-z0-9]+/).filter(w => w.length >= 3);
            let full = lower.replace(/[^a-z0-9]/g, "");
            if (full.length >= 3 && words.indexOf(full) === -1) words.push(full);
            let splitCamel = str.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase().split(/[^a-z0-9]+/).filter(w => w.length >= 3);
            for (let w of splitCamel) if (words.indexOf(w) === -1) words.push(w);
            return words;
        }}

        function matches(target, iconText) {{
            let tTokens = getTokens(target);
            let iTokens = getTokens(iconText);
            let iFull = iconText.toLowerCase().replace(/[^a-z0-9]/g, "");
            for (let t of tTokens) {{
                if (iFull.indexOf(t) !== -1) return true;
                for (let it of iTokens) {{
                    if (it === t) return true;
                    if (t.length >= 4 && it.indexOf(t) !== -1) return true;
                    if (it.length >= 4 && t.indexOf(it) !== -1) return true;
                }}
            }}
            return false;
        }}

        let modified = [];
        if (!Main || !Main.panelManager) return JSON.stringify(modified);
        let panels = Main.panelManager.panels || [];
        for (let p of panels) {{
            if (!p) continue;
            for (let box of [p._leftBox, p._centerBox, p._rightBox]) {{
                if (!box) continue;
                for (let c of box.get_children()) {{
                    if (c._applet && c._applet._uuid === "xapp-status@cinnamon.org") {{
                        let applet = c._applet;
                        for (let k in applet.statusIcons) {{
                            let it = applet.statusIcons[k];
                            if (!it || !it.actor) continue;
                            let text = (k + " " + (it.name || "") + " " + (it.iconName || "") + " " + (it.tooltip || ""));
                            let matched = targets.some(t => matches(t, text));
                            if (matched) {{
                                it.actor.hide();
                                modified.push(k);
                            }}
                        }}
                    }}
                }}
            }}
        }}
        return JSON.stringify(modified);
    }} catch (e) {{
        return JSON.stringify([]);
    }}
}})()
"#
    );

    match eval_cinnamon(&script) {
        Ok(json_str) => serde_json::from_str::<Vec<String>>(&json_str).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Показывает ранее скрытые иконки в системном трее Cinnamon.
pub fn show_tray_items(keys: &[String]) {
    if keys.is_empty() {
        return;
    }
    let keys_json = serde_json::to_string(keys).unwrap_or_else(|_| "[]".into());
    let script = format!(
        r#"
(() => {{
    try {{
        let keys = new Set({keys_json});
        if (!Main || !Main.panelManager) return;
        let panels = Main.panelManager.panels || [];
        for (let p of panels) {{
            if (!p) continue;
            for (let box of [p._leftBox, p._centerBox, p._rightBox]) {{
                if (!box) continue;
                for (let c of box.get_children()) {{
                    if (c._applet && c._applet._uuid === "xapp-status@cinnamon.org") {{
                        let applet = c._applet;
                        for (let k in applet.statusIcons) {{
                            if (keys.has(k)) {{
                                let it = applet.statusIcons[k];
                                if (it && it.actor) it.actor.show();
                            }}
                        }}
                    }}
                }}
            }}
        }}
    }} catch (e) {{}}
}})()
"#
    );
    let _ = eval_cinnamon(&script);
}

/// Гарантированно восстанавливает ВСЕ иконки в трее (при выходе или старте).
pub fn restore_all_tray() {
    let script = r#"
(() => {
    try {
        if (!Main || !Main.panelManager) return;
        let panels = Main.panelManager.panels || [];
        for (let p of panels) {
            if (!p) continue;
            for (let box of [p._leftBox, p._centerBox, p._rightBox]) {
                if (!box) continue;
                for (let c of box.get_children()) {
                    if (c._applet && c._applet._uuid === "xapp-status@cinnamon.org") {
                        let applet = c._applet;
                        for (let k in applet.statusIcons) {
                            let it = applet.statusIcons[k];
                            if (it && it.actor) it.actor.show();
                        }
                    }
                }
            }
        }
    } catch (e) {}
})()
"#;
    let _ = eval_cinnamon(script);
}
