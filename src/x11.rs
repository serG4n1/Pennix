use serde::{Deserialize, Serialize};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        _NET_CLIENT_LIST,
        _NET_CLIENT_LIST_STACKING,
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WM_ICON,
        _NET_WM_STATE,
        _NET_WM_STATE_HIDDEN,
        _NET_WM_STATE_FOCUSED,
        _NET_WM_STATE_FULLSCREEN,
        _NET_WM_STATE_MAXIMIZED_VERT,
        _NET_WM_STATE_MAXIMIZED_HORZ,
        _NET_WM_DESKTOP,
        _NET_MOVERESIZE_WINDOW,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_DESKTOP,
        WM_STATE,
        WM_CHANGE_STATE,
        UTF8_STRING,
    }
}

/// Информация об окне верхнего уровня.
#[derive(Clone, Debug)]
pub struct WinInfo {
    pub id: Window,
    pub title: String,
    pub class: String,
    pub pid: Option<u32>,
}

/// Иконка окна в RGBA.
pub struct Icon {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

/// Всё, что нужно, чтобы вернуть спрятанное окно в прежнее состояние.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HiddenWin {
    pub id: u32,
    pub x: i16,
    pub y: i16,
    pub desktop: Option<u32>,
    pub states: Vec<u32>,
    pub iconic: bool,
}

pub struct X {
    pub conn: RustConnection,
    pub root: Window,
    pub atoms: Atoms,
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

impl X {
    pub fn connect() -> Result<Self, String> {
        let (conn, screen) = x11rb::connect(None)
            .map_err(|e| format!("Не удалось подключиться к X-серверу: {e}"))?;
        let root = conn.setup().roots[screen].root;
        let atoms = Atoms::new(&conn).map_err(err)?.reply().map_err(err)?;
        Ok(Self { conn, root, atoms })
    }

    fn prop32(&self, win: Window, prop: Atom, ty: impl Into<Atom>, len: u32) -> Option<Vec<u32>> {
        let r = self
            .conn
            .get_property(false, win, prop, ty, 0, len)
            .ok()?
            .reply()
            .ok()?;
        r.value32().map(|it| it.collect())
    }

    fn prop_bytes(&self, win: Window, prop: impl Into<Atom>, ty: impl Into<Atom>) -> Option<Vec<u8>> {
        let r = self
            .conn
            .get_property(false, win, prop, ty, 0, 4096)
            .ok()?
            .reply()
            .ok()?;
        if r.value.is_empty() { None } else { Some(r.value) }
    }

    /// Окна в порядке наложения (снизу вверх).
    pub fn client_list(&self) -> Vec<Window> {
        self.prop32(self.root, self.atoms._NET_CLIENT_LIST_STACKING, AtomEnum::WINDOW, 1 << 16)
            .filter(|v| !v.is_empty())
            .or_else(|| self.prop32(self.root, self.atoms._NET_CLIENT_LIST, AtomEnum::WINDOW, 1 << 16))
            .unwrap_or_default()
    }

    pub fn active_window(&self) -> Option<Window> {
        self.prop32(self.root, self.atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW, 1)
            .and_then(|v| v.first().copied())
            .filter(|&w| w != 0)
    }

    pub fn class(&self, win: Window) -> String {
        let raw = self
            .prop_bytes(win, AtomEnum::WM_CLASS, AtomEnum::STRING)
            .unwrap_or_default();
        let parts: Vec<String> = raw
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        parts.get(1).or(parts.first()).cloned().unwrap_or_default()
    }

    pub fn title(&self, win: Window) -> String {
        self.prop_bytes(win, self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING)
            .or_else(|| self.prop_bytes(win, AtomEnum::WM_NAME, AtomEnum::ANY))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    pub fn pid(&self, win: Window) -> Option<u32> {
        self.prop32(win, self.atoms._NET_WM_PID, AtomEnum::CARDINAL, 1)
            .and_then(|v| v.first().copied())
    }

    fn is_special(&self, win: Window) -> bool {
        let types = self
            .prop32(win, self.atoms._NET_WM_WINDOW_TYPE, AtomEnum::ATOM, 16)
            .unwrap_or_default();
        types.iter().any(|&t| {
            t == self.atoms._NET_WM_WINDOW_TYPE_DOCK || t == self.atoms._NET_WM_WINDOW_TYPE_DESKTOP
        })
    }

    /// Все обычные окна (без панелей и рабочего стола).
    pub fn list_windows(&self) -> Vec<WinInfo> {
        self.client_list()
            .into_iter()
            .filter(|&w| !self.is_special(w))
            .map(|id| WinInfo { id, title: self.title(id), class: self.class(id), pid: self.pid(id) })
            .filter(|w| !w.class.is_empty())
            .collect()
    }

    /// Иконка окна, размер ближе всего к `want`.
    pub fn icon(&self, win: Window, want: usize) -> Option<Icon> {
        let data = self.prop32(win, self.atoms._NET_WM_ICON, AtomEnum::CARDINAL, 1 << 20)?;
        let mut best: Option<(usize, usize, usize)> = None; // (w, h, offset)
        let mut i = 0;
        while i + 2 <= data.len() {
            let (w, h) = (data[i] as usize, data[i + 1] as usize);
            if w == 0 || h == 0 || i + 2 + w * h > data.len() {
                break;
            }
            let better = match best {
                None => true,
                Some((bw, _, _)) => {
                    (w >= want && (bw < want || w < bw)) || (bw < want && w > bw)
                }
            };
            if better {
                best = Some((w, h, i + 2));
            }
            i += 2 + w * h;
        }
        let (w, h, off) = best?;
        let mut rgba = Vec::with_capacity(w * h * 4);
        for &p in &data[off..off + w * h] {
            rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
        }
        Some(Icon { w, h, rgba })
    }
}
