//! Главное окно приложения Pennix.

mod panels;

use crate::config::{load_hidden, load_hidden_tray, save_hidden, save_hidden_tray, Config, Hotkey};
use crate::engine::{Engine, Selection, Status};
use crate::theme::*;
use crate::x11::X;
use eframe::egui::{self, vec2, Color32, RichText, Stroke, TextureHandle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct AppEntry {
    pub class: String,
    pub titles: Vec<String>,
    pub in_tray: bool,
}

#[derive(Clone, Debug)]
pub struct WindowEntry {
    pub id: u32,
    pub title: String,
    pub class: String,
    pub pid: Option<u32>,
    pub is_alive: bool,
}

pub struct PennixApp {
    pub cfg: Config,
    pub x: Option<X>,
    pub x_error: Option<String>,
    pub apps: Vec<AppEntry>,
    pub windows: Vec<WindowEntry>,
    pub icons: HashMap<String, Option<TextureHandle>>,
    pub logo: Option<TextureHandle>,
    pub last_refresh: Instant,
    pub search: String,
    pub recording: bool,
    pub engine: Option<Engine>,
    pub selection: Arc<Mutex<Selection>>,
    pub status: Arc<Mutex<Status>>,
    pub start_error: Option<String>,
    pub wayland: bool,
}

impl PennixApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup(&cc.egui_ctx);
        let cfg = Config::load();
        let (x, x_error) = match X::connect() {
            Ok(x) => (Some(x), None),
            Err(e) => (None, Some(e)),
        };

        // Аварийное восстановление окон и трея, если прошлый сеанс упал
        if let Some(x) = &x {
            let left_wins = load_hidden();
            if !left_wins.is_empty() {
                x.show_windows(&left_wins, None);
                save_hidden(&[]);
            }
        }
        let left_tray = load_hidden_tray();
        if !left_tray.is_empty() {
            crate::tray::show_tray_items(&left_tray);
            crate::tray::restore_all_tray();
            save_hidden_tray(&[]);
        }

        let logo = image::load_from_memory(include_bytes!("../assets/pennix.png")).ok().map(|img| {
            let img = img.to_rgba8();
            let ci = egui::ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                &img,
            );
            cc.egui_ctx.load_texture("logo", ci, egui::TextureOptions::LINEAR)
        });

        let selection = Arc::new(Mutex::new(Selection {
            mode: cfg.mode,
            classes: cfg.classes.clone(),
            windows: cfg.windows.clone(),
            hide_self: cfg.hide_self,
            hide_tray: cfg.hide_tray,
        }));

        let wayland = std::env::var("XDG_SESSION_TYPE").map(|s| s == "wayland").unwrap_or(false);

        let mut app = Self {
            cfg,
            x,
            x_error,
            apps: vec![],
            windows: vec![],
            icons: HashMap::new(),
            logo,
            last_refresh: Instant::now(),
            search: String::new(),
            recording: false,
            engine: None,
            selection,
            status: Arc::new(Mutex::new(Status::default())),
            start_error: None,
            wayland,
        };
        app.refresh(&cc.egui_ctx);
        app
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        self.last_refresh = Instant::now();
        let own = std::process::id();

        // 1. Опрашиваем окна X11
        let mut map: Vec<(String, Vec<String>, u32)> = Vec::new();
        let mut win_entries: Vec<WindowEntry> = Vec::new();

        if let Some(x) = &self.x {
            for w in x
                .list_windows()
                .into_iter()
                .filter(|w| w.pid != Some(own) && w.class != "pennix" && w.class != "pennix-linux")
            {
                let display_title = if w.title.trim().is_empty() {
                    format!("<Без названия: {}>", w.class)
                } else {
                    w.title.clone()
                };

                // Для группировки по приложениям
                match map.iter_mut().find(|e| e.0 == w.class) {
                    Some(e) => e.1.push(display_title.clone()),
                    None => map.push((w.class.clone(), vec![display_title.clone()], w.id)),
                }

                // Для списка отдельных окон / процессов
                win_entries.push(WindowEntry {
                    id: w.id,
                    title: display_title.clone(),
                    class: w.class.clone(),
                    pid: w.pid,
                    is_alive: true,
                });

                // Если заголовок окна изменился (например, переключили вкладку), обновим в сохранённых
                for saved in &mut self.cfg.windows {
                    if saved.id != 0 && saved.id == w.id && saved.title != display_title {
                        saved.title = display_title.clone();
                    }
                }
            }

            // Извлекаем иконки окон X11
            for (class, _, win) in &map {
                if !self.icons.contains_key(class) || self.icons[class].is_none() {
                    let tex = x.icon(*win, 48).map(|ic| {
                        let ci = egui::ColorImage::from_rgba_unmultiplied([ic.w, ic.h], &ic.rgba);
                        ctx.load_texture(format!("icon-{class}"), ci, egui::TextureOptions::LINEAR)
                    });
                    self.icons.insert(class.clone(), tex);
                }
            }
        }

        // Сохраняем ранее выбранные окна, если они сейчас скрыты или закрыты
        for saved in &self.cfg.windows {
            let already = win_entries.iter().any(|w| {
                (saved.id != 0 && w.id == saved.id)
                    || (w.class == saved.class && w.title == saved.title)
            });
            if !already {
                win_entries.push(WindowEntry {
                    id: saved.id,
                    title: saved.title.clone(),
                    class: saved.class.clone(),
                    pid: saved.pid,
                    is_alive: false,
                });
            }
        }

        win_entries.sort_by(|a, b| {
            a.class
                .to_lowercase()
                .cmp(&b.class.to_lowercase())
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        self.windows = win_entries;

        // 2. Опрашиваем системный трей (StatusNotifierItem / XApp)
        let tray_items = crate::tray::list_tray_items();
        let mut app_entries: Vec<AppEntry> = Vec::new();

        for (class, titles, _) in map {
            let cl = class.to_lowercase();
            let has_tray = tray_items.iter().any(|t| {
                let tc = t.class.to_lowercase();
                let tt = t.title.to_lowercase();
                let ti = t.id.to_lowercase();
                tc == cl
                    || tt == cl
                    || ti == cl
                    || cl.contains(&tc)
                    || tc.contains(&cl)
                    || (!tt.is_empty() && (cl.contains(&tt) || tt.contains(&cl)))
                    || (!ti.is_empty() && (cl.contains(&ti) || ti.contains(&cl)))
            });
            app_entries.push(AppEntry { class, titles, in_tray: has_tray });
        }

        // 3. Добавляем приложения, которые висят ТОЛЬКО в трее (без X11 окна)
        for tray in &tray_items {
            let tc = tray.class.to_lowercase();
            let tt = tray.title.to_lowercase();
            let ti = tray.id.to_lowercase();
            let already = app_entries.iter().any(|a| {
                let ac = a.class.to_lowercase();
                ac == tc
                    || ac == tt
                    || ac == ti
                    || ac.contains(&tc)
                    || tc.contains(&ac)
                    || (!tt.is_empty() && (ac.contains(&tt) || tt.contains(&ac)))
                    || (!ti.is_empty() && (ac.contains(&ti) || ti.contains(&ac)))
            });
            if !already {
                // Пытаемся загрузить иконку из файла трея
                if !self.icons.contains_key(&tray.class) || self.icons[&tray.class].is_none() {
                    if let Some(file_path) = &tray.icon_file {
                        if let Ok(img) = image::open(file_path) {
                            let img = img.to_rgba8();
                            let ci = egui::ColorImage::from_rgba_unmultiplied(
                                [img.width() as usize, img.height() as usize],
                                &img,
                            );
                            let tex = ctx.load_texture(
                                format!("tray-icon-{}", tray.class),
                                ci,
                                egui::TextureOptions::LINEAR,
                            );
                            self.icons.insert(tray.class.clone(), Some(tex));
                        }
                    }
                }

                let title = if !tray.title.is_empty() {
                    tray.title.clone()
                } else {
                    tray.class.clone()
                };

                app_entries.push(AppEntry {
                    class: tray.class.clone(),
                    titles: vec![format!("{title} (в трее)")],
                    in_tray: true,
                });
            }
        }

        // 4. Добавляем выбранные ранее классы, если их сейчас нет ни в окнах, ни в трее
        for c in &self.cfg.classes {
            if !app_entries.iter().any(|a| &a.class == c) {
                app_entries.push(AppEntry {
                    class: c.clone(),
                    titles: vec![],
                    in_tray: false,
                });
            }
        }

        app_entries.sort_by_key(|a| a.class.to_lowercase());
        self.apps = app_entries;
    }

    pub fn sync_selection(&mut self) {
        *self.selection.lock().unwrap() = Selection {
            mode: self.cfg.mode,
            classes: self.cfg.classes.clone(),
            windows: self.cfg.windows.clone(),
            hide_self: self.cfg.hide_self,
            hide_tray: self.cfg.hide_tray,
        };
        self.cfg.save();
    }

    fn start(&mut self, ctx: &egui::Context) {
        let c = ctx.clone();
        match Engine::start(
            &self.cfg.hotkey,
            self.selection.clone(),
            self.status.clone(),
            move || c.request_repaint(),
        ) {
            Ok(e) => {
                self.engine = Some(e);
                self.start_error = None;
            }
            Err(e) => self.start_error = Some(e),
        }
    }

    fn stop(&mut self) {
        if let Some(mut e) = self.engine.take() {
            e.stop();
        }
    }

    fn capture_hotkey(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            if let egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } = ev {
                if key == egui::Key::Escape && modifiers.is_none() {
                    self.recording = false;
                    return;
                }
                if let Some(h) = Hotkey::from_egui(physical_key.unwrap_or(key), modifiers) {
                    self.cfg.hotkey = h;
                    self.cfg.save();
                    self.recording = false;
                    return;
                }
            }
        }
    }
}

impl eframe::App for PennixApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.recording {
            self.capture_hotkey(&ctx);
        }
        let status = self.status.lock().unwrap().clone();
        if self.engine.is_some() && !status.running {
            self.engine = None;
            self.start_error = status.error.clone();
        }
        if self.last_refresh.elapsed().as_secs_f32() > 2.5 {
            self.refresh(&ctx);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(1000));

        egui::Frame::new().fill(BG).inner_margin(22).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui, &status);
            ui.add_space(14.0);
            if let Some(e) = &self.x_error {
                banner(ui, RED, &format!("{e}. Pennix работает в сеансе X11."));
            } else if self.wayland {
                banner(
                    ui,
                    AMBER,
                    "Сеанс Wayland: прятать можно XWayland-окна и иконки трея. Для полного сокрытия переключитесь на X11.",
                );
            }
            let h = ui.available_height();
            let w = ui.available_width();
            let left = (w * 0.56).max(320.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    vec2(left, h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        card(ui, |ui| {
                            ui.set_min_size(vec2(ui.available_width(), h - 38.0));
                            self.apps_panel(ui);
                        });
                    },
                );
                ui.add_space(6.0);
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        card(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.hotkey_panel(ui, &status);
                        });
                        ui.add_space(6.0);
                        card(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_min_height(ui.available_height() - 38.0);
                            self.run_panel(ui, &status, &ctx);
                        });
                    },
                );
            });
        });
    }
}

fn banner(ui: &mut egui::Ui, color: Color32, text: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 28))
        .stroke(Stroke::new(1.0, color))
        .corner_radius(10)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(color));
        });
    ui.add_space(10.0);
}
