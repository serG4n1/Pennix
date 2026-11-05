//! Фоновый поток: ловит горячую клавишу и прячет/показывает окна и иконки в трее.

use crate::config::{save_hidden, Hotkey};
use crate::tray::{hide_tray_for_classes, restore_tray_icons};
use crate::x11::{HiddenWin, X};
use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::Event;

#[derive(Clone, Default)]
pub struct Selection {
    pub classes: Vec<String>,
    pub hide_self: bool,
    pub hide_tray: bool,
}

#[derive(Clone, Default)]
pub struct Status {
    pub running: bool,
    pub is_hidden: bool,
    pub hidden_count: usize,
    pub hidden_tray_count: usize,
    pub error: Option<String>,
}

pub enum Cmd {
    Toggle,
    Stop,
}

pub struct Engine {
    tx: Sender<Cmd>,
    handle: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn start(
        hotkey: &Hotkey,
        selection: Arc<Mutex<Selection>>,
        status: Arc<Mutex<Status>>,
        repaint: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let x = X::connect()?;
        let codes = x.keycodes_for(hotkey.keysym);
        if codes.is_empty() {
            return Err(format!("Клавиша «{}» не найдена на клавиатуре.", hotkey.key_name));
        }
        let mods = hotkey.x_mods();
        x.grab(&codes, mods)?;
        x.detectable_autorepeat();
        let _ = x.conn.flush();

        *status.lock().unwrap() = Status {
            running: true,
            is_hidden: false,
            hidden_count: 0,
            hidden_tray_count: 0,
            error: None,
        };
        let (tx, rx) = channel();
        let handle = std::thread::spawn(move || {
            let own_pid = std::process::id();
            let mut hidden: Vec<HiddenWin> = Vec::new();
            let mut hidden_tray: Vec<String> = Vec::new();
            let mut is_hidden = false;
            let mut focus = None;
            let mut key_down = false;
            let mut last_scan = Instant::now();

            let hide_new = |x: &X, hidden: &mut Vec<HiddenWin>, hidden_tray: &mut Vec<String>| {
                let sel = selection.lock().unwrap().clone();
                let self_pid = sel.hide_self.then_some(own_pid);
                for w in x.windows_matching(&sel.classes, self_pid, own_pid) {
                    if !hidden.iter().any(|h| h.id == w) {
                        if let Some(h) = x.hide_window(w) {
                            hidden.push(h);
                        }
                    }
                }
                let _ = x.conn.flush();
                save_hidden(hidden);

                if sel.hide_tray {
                    let newly_hidden = hide_tray_for_classes(&sel.classes);
                    for r in newly_hidden {
                        if !hidden_tray.iter().any(|existing| existing.eq_ignore_ascii_case(&r)) {
                            hidden_tray.push(r);
                        }
                    }
                }
            };

            let toggle = |x: &X,
                          hidden: &mut Vec<HiddenWin>,
                          hidden_tray: &mut Vec<String>,
                          is_hidden: &mut bool,
                          focus: &mut Option<u32>| {
                if !*is_hidden {
                    *focus = x.active_window();
                    hide_new(x, hidden, hidden_tray);
                    *is_hidden = !hidden.is_empty() || !hidden_tray.is_empty();
                } else {
                    x.show_windows(hidden, *focus);
                    hidden.clear();
                    save_hidden(hidden);
                    restore_tray_icons();
                    hidden_tray.clear();
                    *is_hidden = false;
                }
                let mut st = status.lock().unwrap();
                st.is_hidden = *is_hidden;
                st.hidden_count = hidden.len();
                st.hidden_tray_count = hidden_tray.len();
                drop(st);
                repaint();
            };

            loop {
                loop {
                    match x.conn.poll_for_event() {
                        Ok(Some(Event::KeyPress(_))) => {
                            if !key_down {
                                key_down = true;
                                toggle(&x, &mut hidden, &mut hidden_tray, &mut is_hidden, &mut focus);
                            }
                        }
                        Ok(Some(Event::KeyRelease(_))) => key_down = false,
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(e) => {
                            x.show_windows(&hidden, focus);
                            save_hidden(&[]);
                            restore_tray_icons();
                            *status.lock().unwrap() = Status {
                                running: false,
                                is_hidden: false,
                                hidden_count: 0,
                                hidden_tray_count: 0,
                                error: Some(format!("Связь с X-сервером потеряна: {e}")),
                            };
                            repaint();
                            return;
                        }
                    }
                }
                match rx.try_recv() {
                    Ok(Cmd::Toggle) => toggle(&x, &mut hidden, &mut hidden_tray, &mut is_hidden, &mut focus),
                    Ok(Cmd::Stop) | Err(TryRecvError::Disconnected) => break,
                    Err(TryRecvError::Empty) => {}
                }
                // Пока режим скрытия активен — прячем и вновь открывшиеся окна выбранных приложений.
                if is_hidden && last_scan.elapsed() > Duration::from_millis(500) {
                    last_scan = Instant::now();
                    let sel = selection.lock().unwrap().clone();
                    let self_pid = sel.hide_self.then_some(own_pid);
                    let mut changed = false;
                    for w in x.windows_matching(&sel.classes, self_pid, own_pid) {
                        if !hidden.iter().any(|h| h.id == w) {
                            if let Some(h) = x.hide_window(w) {
                                hidden.push(h);
                                changed = true;
                            }
                        }
                    }
                    if changed {
                        let _ = x.conn.flush();
                        save_hidden(&hidden);
                        let mut st = status.lock().unwrap();
                        st.hidden_count = hidden.len();
                        drop(st);
                        repaint();
                    }
                }
                std::thread::sleep(Duration::from_millis(15));
            }

            x.show_windows(&hidden, focus);
            save_hidden(&[]);
            restore_tray_icons();
            x.ungrab(&codes, mods);
            *status.lock().unwrap() = Status::default();
            repaint();
        });
        Ok(Self { tx, handle: Some(handle) })
    }

    pub fn toggle(&self) {
        let _ = self.tx.send(Cmd::Toggle);
    }

    pub fn stop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}
