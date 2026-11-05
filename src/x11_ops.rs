//! Скрытие/восстановление окон и глобальные горячие клавиши.

use crate::config::WindowTarget;
use crate::x11::{HiddenWin, X};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::CURRENT_TIME;

/// Варианты Lock/NumLock, которые нужно игнорировать при захвате клавиши.
const IGNORED_MODS: [u16; 4] = [0, 2, 16, 2 | 16];

impl X {
    fn wm_state(&self, win: Window) -> Option<u32> {
        let r = self
            .conn
            .get_property(false, win, self.atoms.WM_STATE, self.atoms.WM_STATE, 0, 2)
            .ok()?
            .reply()
            .ok()?;
        r.value32().and_then(|mut it| it.next())
    }

    fn prop_u32s(&self, win: Window, prop: Atom, ty: impl Into<Atom>) -> Vec<u32> {
        self.conn
            .get_property(false, win, prop, ty, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|it| it.collect()))
            .unwrap_or_default()
    }

    fn root_msg(&self, win: Window, ty: Atom, data: [u32; 5]) {
        let ev = ClientMessageEvent::new(32, win, ty, data);
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
    }

    /// Окна выбранных классов (+ окна процесса `self_pid`, если задан).
    pub fn windows_matching(&self, classes: &[String], self_pid: Option<u32>, own_pid: u32) -> Vec<Window> {
        self.list_windows()
            .into_iter()
            .filter(|w| {
                let mine = w.pid == Some(own_pid) || w.class == "pennix" || w.class == "pennix-linux";
                if mine {
                    self_pid.is_some()
                } else {
                    classes.iter().any(|c| c == &w.class)
                }
            })
            .map(|w| w.id)
            .collect()
    }

    /// Окна конкретных выбранных окон/вкладок (+ окна `self_pid`, если задан).
    pub fn windows_matching_targets(
        &self,
        targets: &[WindowTarget],
        self_pid: Option<u32>,
        own_pid: u32,
    ) -> Vec<Window> {
        self.list_windows()
            .into_iter()
            .filter(|w| {
                let mine = w.pid == Some(own_pid) || w.class == "pennix" || w.class == "pennix-linux";
                if mine {
                    self_pid.is_some()
                } else {
                    targets.iter().any(|t| {
                        if t.id != 0 && w.id == t.id {
                            return true;
                        }
                        if !t.title.is_empty() && t.class == w.class && t.title == w.title {
                            return true;
                        }
                        if let (Some(t_pid), Some(w_pid)) = (t.pid, w.pid) {
                            if t_pid != 0 && t_pid == w_pid && t.class == w.class && (w.title.contains(&t.title) || t.title.contains(&w.title)) {
                                return true;
                            }
                        }
                        false
                    })
                }
            })
            .map(|w| w.id)
            .collect()
    }

    /// Прячет окно: снимает с экрана и «отзывает» у оконного менеджера
    /// (ICCCM withdraw) — окно пропадает с панели задач и из Alt+Tab.
    pub fn hide_window(&self, win: Window) -> Option<HiddenWin> {
        let pos = self.conn.translate_coordinates(win, self.root, 0, 0).ok()?.reply().ok()?;
        let desktop = self
            .prop_u32s(win, self.atoms._NET_WM_DESKTOP, AtomEnum::CARDINAL)
            .first()
            .copied();
        let a = &self.atoms;
        let states: Vec<u32> = self
            .prop_u32s(win, a._NET_WM_STATE, AtomEnum::ATOM)
            .into_iter()
            .filter(|&s| s != a._NET_WM_STATE_HIDDEN && s != a._NET_WM_STATE_FOCUSED)
            .collect();
        let iconic = self.wm_state(win) == Some(3);

        self.conn.unmap_window(win).ok()?;
        let ev = UnmapNotifyEvent {
            response_type: UNMAP_NOTIFY_EVENT,
            sequence: 0,
            event: self.root,
            window: win,
            from_configure: false,
        };
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
        Some(HiddenWin { id: win, x: pos.dst_x, y: pos.dst_y, desktop, states, iconic })
    }

    /// Возвращает окна на место: рабочий стол, развёрнутость, позицию, фокус.
    pub fn show_windows(&self, list: &[HiddenWin], focus: Option<Window>) {
        let a = &self.atoms;
        let mut alive = Vec::new();
        for h in list {
            if self.conn.get_window_attributes(h.id).ok().and_then(|c| c.reply().ok()).is_none() {
                continue; // окно уже закрыто
            }
            if let Some(d) = h.desktop {
                let _ = self.conn.change_property32(PropMode::REPLACE, h.id, a._NET_WM_DESKTOP, AtomEnum::CARDINAL, &[d]);
            }
            let _ = self.conn.change_property32(PropMode::REPLACE, h.id, a._NET_WM_STATE, AtomEnum::ATOM, &h.states);
            let _ = self.conn.configure_window(h.id, &ConfigureWindowAux::new().x(h.x as i32).y(h.y as i32));
            let _ = self.conn.map_window(h.id);
            alive.push(h);
        }
        let _ = self.conn.flush();
        if alive.is_empty() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(120));

        for h in &alive {
            let maximized = h.states.iter().any(|&s| {
                s == a._NET_WM_STATE_FULLSCREEN
                    || s == a._NET_WM_STATE_MAXIMIZED_VERT
                    || s == a._NET_WM_STATE_MAXIMIZED_HORZ
            });
            if !maximized {
                // StaticGravity (10) + x + y, источник — «пейджер» (2)
                let flags = 10 | (1 << 8) | (1 << 9) | (2 << 12);
                self.root_msg(h.id, a._NET_MOVERESIZE_WINDOW, [flags, h.x as i32 as u32, h.y as i32 as u32, 0, 0]);
            }
            if h.iconic {
                self.root_msg(h.id, a.WM_CHANGE_STATE, [3, 0, 0, 0, 0]);
            }
        }
        if let Some(f) = focus.filter(|f| alive.iter().any(|h| h.id == *f && !h.iconic)) {
            self.root_msg(f, a._NET_ACTIVE_WINDOW, [2, CURRENT_TIME, 0, 0, 0]);
        }
        let _ = self.conn.flush();
    }

    /// Все keycode, которые дают данный keysym (в любой раскладке).
    pub fn keycodes_for(&self, keysym: u32) -> Vec<Keycode> {
        let setup = self.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let Some(r) = self
            .conn
            .get_keyboard_mapping(min, max - min + 1)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return vec![];
        };
        let per = r.keysyms_per_keycode.max(1) as usize;
        let lower = if (0x41..=0x5a).contains(&keysym) { keysym + 0x20 } else { keysym };
        r.keysyms
            .chunks(per)
            .enumerate()
            .filter(|(_, ks)| ks.iter().any(|&k| k == keysym || k == lower))
            .map(|(i, _)| min + i as u8)
            .collect()
    }

    /// Захватывает комбинацию глобально. Ошибка — если она занята.
    pub fn grab(&self, codes: &[Keycode], mods: u16) -> Result<(), String> {
        for &kc in codes {
            for ign in IGNORED_MODS {
                let res = self
                    .conn
                    .grab_key(false, self.root, ModMask::from(mods | ign), kc, GrabMode::ASYNC, GrabMode::ASYNC)
                    .map_err(|e| e.to_string())?
                    .check();
                if res.is_err() {
                    self.ungrab(codes, mods);
                    return Err("Эта комбинация уже занята другим приложением или системой. Выберите другую.".into());
                }
            }
        }
        Ok(())
    }

    pub fn ungrab(&self, codes: &[Keycode], mods: u16) {
        for &kc in codes {
            for ign in IGNORED_MODS {
                let _ = self.conn.ungrab_key(kc, self.root, ModMask::from(mods | ign));
            }
        }
        let _ = self.conn.flush();
    }

    /// Чтобы при удержании клавиши не было «дребезга» скрыть/показать.
    pub fn detectable_autorepeat(&self) {
        use x11rb::protocol::xkb::{self, ConnectionExt as _};
        if self.conn.xkb_use_extension(1, 0).ok().and_then(|c| c.reply().ok()).is_none() {
            return;
        }
        let f = xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT;
        let _ = self
            .conn
            .xkb_per_client_flags(xkb::ID::USE_CORE_KBD.into(), f, f, Default::default(), Default::default(), Default::default())
            .ok()
            .and_then(|c| c.reply().ok());
    }
}
