//! Фоновый поток Pennix: ловит горячую клавишу, прячет/показывает X11 окна и иконки трея.

use crate::config::{save_hidden, save_hidden_tray, Hotkey};
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
    pub hidden_count: usize,
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

        *status.lock().unwrap() = Status { running: true, hidden_count: 0, error: None };
        let (tx, rx) = channel();
        let handle = std::thread::spawn(move || {
            let own_pid = std::process::id();
            let mut hidden_windows: Vec<HiddenWin> = Vec::new();
            let mut hidden_tray: Vec<String> = Vec::new();
            let mut focus = None;
            let mut key_down = false;
            let mut last_scan = Instant::now();

            let hide_new = |x: &X, hidden_w: &mut Vec<HiddenWin>, hidden_t: &mut Vec<String>| {
                let sel = selection.lock().unwrap().clone();
                let self_pid = sel.hide_self.then_some(own_pid);

                // 1. Скрываем X11 окна
                for w in x.windows_matching(&sel.classes, self_pid, own_pid) {
                    if !hidden_w.iter().any(|h| h.id == w) {
                        if let Some(h) = x.hide_window(w) {
                            hidden_w.push(h);
                        }
                    }
                }
                let _ = x.conn.flush();
                save_hidden(hidden_w);

                // 2. Скрываем иконки трея (StatusNotifierItem / XApp)
                if sel.hide_tray {
                    let new_tray = crate::tray::hide_tray_items(&sel.classes);
                    for k in new_tray {
                        if !hidden_t.contains(&k) {
                            hidden_t.push(k);
                        }
                    }
                    save_hidden_tray(hidden_t);
                }
            };

            let toggle = |x: &X,
                          hidden_w: &mut Vec<HiddenWin>,
                          hidden_t: &mut Vec<String>,
                          focus: &mut Option<u32>| {
                let is_hidden = !hidden_w.is_empty() || !hidden_t.is_empty();
                if !is_hidden {
                    *focus = x.active_window();
                    hide_new(x, hidden_w, hidden_t);
                } else {
                    x.show_windows(hidden_w, *focus);
                    hidden_w.clear();
                    save_hidden(hidden_w);

                    if !hidden_t.is_empty() {
                        crate::tray::show_tray_items(hidden_t);
                        hidden_t.clear();
                        save_hidden_tray(hidden_t);
                    }
                }
                status.lock().unwrap().hidden_count = hidden_w.len() + hidden_t.len();
                repaint();
            };

            loop {
                loop {
                    match x.conn.poll_for_event() {
                        Ok(Some(Event::KeyPress(_))) => {
                            if !key_down {
                                key_down = true;
                                toggle(&x, &mut hidden_windows, &mut hidden_tray, &mut focus);
                            }
                        }
                        Ok(Some(Event::KeyRelease(_))) => key_down = false,
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(e) => {
                            x.show_windows(&hidden_windows, focus);
                            save_hidden(&[]);
                            if !hidden_tray.is_empty() {
                                crate::tray::show_tray_items(&hidden_tray);
                                save_hidden_tray(&[]);
                            }
                            *status.lock().unwrap() = Status {
                                running: false,
                                hidden_count: 0,
                                error: Some(format!("Связь с X-сервером потеряна: {e}")),
                            };
                            repaint();
                            return;
                        }
                    }
                }
                match rx.try_recv() {
                    Ok(Cmd::Toggle) => toggle(&x, &mut hidden_windows, &mut hidden_tray, &mut focus),
                    Ok(Cmd::Stop) | Err(TryRecvError::Disconnected) => break,
                    Err(TryRecvError::Empty) => {}
                }
                // Пока окна или трей спрятаны — подхватываем вновь открывающиеся окна
                let is_hidden = !hidden_windows.is_empty() || !hidden_tray.is_empty();
                if is_hidden && last_scan.elapsed() > Duration::from_millis(500) {
                    last_scan = Instant::now();
                    hide_new(&x, &mut hidden_windows, &mut hidden_tray);
                    status.lock().unwrap().hidden_count = hidden_windows.len() + hidden_tray.len();
                }
                std::thread::sleep(Duration::from_millis(15));
            }

            x.show_windows(&hidden_windows, focus);
            save_hidden(&[]);
            if !hidden_tray.is_empty() {
                crate::tray::show_tray_items(&hidden_tray);
                save_hidden_tray(&[]);
            }
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
