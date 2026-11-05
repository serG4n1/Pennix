//! Модуль работы с системным треем (StatusNotifierItem / XAppStatusIcon / XEmbed в Cinnamon).

use crate::x11::Icon;
use serde::{Deserialize, Serialize};
use std::process::Command;

pub const TRAY_UUID: &str = "pennix-linux-tray-hider";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrayAppInfo {
    /// Роль/идентификатор в трее (например, `clash-verge`).
    pub role: String,
    /// Отображаемое имя / подсказка (например, `Clash-verge`).
    pub title: String,
    /// Путь к файлу иконки или имя иконки в теме.
    pub icon_path: String,
}

impl TrayAppInfo {
    /// Каноническое имя класса для отображения в списке приложений (с заглавной буквы, как WM_CLASS).
    pub fn display_class(&self) -> String {
        if !self.title.is_empty() && self.title.eq_ignore_ascii_case(&self.role) {
            return self.title.clone();
        }
        let mut chars = self.role.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => self.role.clone(),
        }
    }
}

/// Проверяет, относятся ли класс окна (`WM_CLASS`) и роль/имя иконки трея к одному приложению.
pub fn class_matches_tray(class: &str, role: &str, title: &str) -> bool {
    let norm = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    };
    let c = norm(class);
    let r = norm(role);
    let t = norm(title);
    if c.is_empty() || r.is_empty() {
        return false;
    }
    c == r || (!t.is_empty() && c == t) || r.contains(&c) || c.contains(&r)
}

/// Выполняет JS-выражение в Cinnamon через D-Bus `org.Cinnamon.Eval` и возвращает строковый результат.
fn cinnamon_eval(js: &str) -> Option<String> {
    let out = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.Cinnamon",
            "/org/Cinnamon",
            "org.Cinnamon",
            "Eval",
            "s",
            js,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // Формат вывода busctl: `bs true "..."`
    let idx = s.find("true \"")?;
    let rest = &s[idx + 5..];
    serde_json::from_str::<String>(rest.trim()).ok()
}

/// Возвращает список всех иконок, присутствующих сейчас в системном трее.
pub fn list_tray_apps() -> Vec<TrayAppInfo> {
    let js = r#"
(() => {
  let out = [];
  let seen = {};
  let xapp = Main.AppletManager.getRunningInstancesForUuid('xapp-status@cinnamon.org');
  if (xapp && xapp[0] && xapp[0].monitor) {
    for (let p of xapp[0].monitor.list_icons()) {
      let role = (p.name || '').trim();
      if (role && !seen[role.toLowerCase()]) {
        seen[role.toLowerCase()] = true;
        out.push({
          role: role,
          title: (p.tooltip_text || p.label || role).trim(),
          icon_path: (p.icon_name || '').trim()
        });
      }
    }
  }
  let sys = Main.AppletManager.getRunningInstancesForUuid('systray@cinnamon.org');
  if (sys && sys[0] && sys[0].button_box) {
    for (let b of sys[0].button_box.get_children()) {
      if (b.child && b.child.wm_class) {
        let role = b.child.wm_class.trim();
        if (role && !seen[role.toLowerCase()]) {
          seen[role.toLowerCase()] = true;
          out.push({
            role: role,
            title: (b.child.title || role).trim(),
            icon_path: ''
          });
        }
      }
    }
  }
  return out;
})()
"#;
    if let Some(json) = cinnamon_eval(js) {
        if let Ok(list) = serde_json::from_str::<Vec<TrayAppInfo>>(&json) {
            return list;
        }
    }
    Vec::new()
}

/// Скрывает в системном трее иконки выбранных классов приложений.
/// Возвращает список скрытых ролей трея.
pub fn hide_tray_for_classes(classes: &[String]) -> Vec<String> {
    if classes.is_empty() {
        return Vec::new();
    }
    let tray_apps = list_tray_apps();
    let mut roles_to_hide = Vec::new();

    for c in classes {
        // Всегда добавляем само имя класса в нижнем регистре на случай классического XEmbed
        let lower_c = c.to_lowercase();
        if !roles_to_hide.contains(&lower_c) {
            roles_to_hide.push(lower_c);
        }
        for ta in &tray_apps {
            if class_matches_tray(c, &ta.role, &ta.title) {
                let r = ta.role.to_lowercase();
                if !roles_to_hide.contains(&r) {
                    roles_to_hide.push(r);
                }
            }
        }
    }

    if roles_to_hide.is_empty() {
        return Vec::new();
    }

    if let Ok(roles_json) = serde_json::to_string(&roles_to_hide) {
        let js = format!(
            r#"(() => {{
                let roles = {roles_json};
                Main.systrayManager.unregisterTrayIconReplacement('{TRAY_UUID}');
                for (let r of roles) {{
                    Main.systrayManager.registerTrayIconReplacement(r, '{TRAY_UUID}');
                }}
                return roles.length;
            }})()"#
        );
        let _ = cinnamon_eval(&js);
    }

    // Возвращаем реально найденные в трее приложения (чтобы счётчик и статус знали, что иконка трея скрыта)
    tray_apps
        .into_iter()
        .filter(|ta| classes.iter().any(|c| class_matches_tray(c, &ta.role, &ta.title)))
        .map(|ta| ta.role)
        .collect()
}

/// Восстанавливает все скрытые иконки в системном трее.
pub fn restore_tray_icons() {
    let js = format!(
        "Main.systrayManager.unregisterTrayIconReplacement('{TRAY_UUID}')"
    );
    let _ = cinnamon_eval(&js);
}

/// Загружает иконку приложения из трея (если `icon_path` указывает на PNG-файл или иконку в стандартных путях).
pub fn load_tray_icon(icon_path: &str) -> Option<Icon> {
    if icon_path.is_empty() {
        return None;
    }
    let path = if icon_path.starts_with('/') {
        std::path::PathBuf::from(icon_path)
    } else {
        let candidates = [
            format!("/usr/share/icons/hicolor/48x48/apps/{icon_path}.png"),
            format!("/usr/share/icons/hicolor/64x64/apps/{icon_path}.png"),
            format!("/usr/share/icons/hicolor/128x128/apps/{icon_path}.png"),
            format!("/usr/share/icons/hicolor/256x256/apps/{icon_path}.png"),
            format!("/usr/share/pixmaps/{icon_path}.png"),
        ];
        candidates
            .into_iter()
            .map(std::path::PathBuf::from)
            .find(|p| p.exists())?
    };

    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Some(Icon {
        w,
        h,
        rgba: img.into_raw(),
    })
}
