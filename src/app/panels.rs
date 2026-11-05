use super::PennixApp;
use crate::config::{SelectionMode, WindowTarget};
use crate::engine::Status;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke};

fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}


impl PennixApp {
    pub(super) fn header(&mut self, ui: &mut egui::Ui, st: &Status) {
        ui.horizontal(|ui| {
            if let Some(logo) = &self.logo {
                let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                ui.painter().image(
                    logo.id(),
                    r,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Pennix").size(26.0).strong());
                    ui.label(RichText::new("для Linux").size(26.0).color(ACCENT2));
                });
                ui.label(
                    RichText::new("Скрытие окон и иконок трея одним нажатием клавиш").color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (text, color) = if st.hidden_count > 0 {
                    (format!("Скрыто объектов: {}", st.hidden_count), AMBER)
                } else if st.running {
                    ("Активно".to_string(), GREEN)
                } else {
                    ("Остановлено".to_string(), MUTED)
                };
                let galley = ui.painter().layout_no_wrap(text, FontId::proportional(14.0), color);
                let (r, _) = ui.allocate_exact_size(vec2(galley.size().x + 40.0, 32.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(
                    r,
                    16,
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 30),
                );
                p.rect_stroke(r, 16, Stroke::new(1.0, color), egui::StrokeKind::Inside);
                let t = ui.input(|i| i.time) as f32;
                let pulse = if st.running { 0.6 + 0.4 * (t * 3.0).sin().abs() } else { 1.0 };
                p.circle_filled(pos2(r.left() + 16.0, r.center().y), 4.5, color.gamma_multiply(pulse));
                p.galley(
                    pos2(r.left() + 28.0, r.center().y - galley.size().y / 2.0),
                    galley,
                    color,
                );
                if st.running {
                    ui.ctx().request_repaint();
                }
            });
        });
    }

    pub(super) fn apps_panel(&mut self, ui: &mut egui::Ui) {
        step_title(ui, 1, "Что скрывать", "Приложения целиком или конкретные окна");
        ui.add_space(8.0);

        // Переключатель режимов: Приложения / Окна
        ui.horizontal(|ui| {
            let total_w = ui.available_width();
            let half = ((total_w - 6.0) / 2.0).max(60.0);
            if mode_pill(ui, "📱  По приложениям", self.cfg.mode == SelectionMode::Apps, half).clicked() {
                self.cfg.mode = SelectionMode::Apps;
                self.sync_selection();
            }
            if mode_pill(ui, "🪟  По окнам / процессам", self.cfg.mode == SelectionMode::Windows, half).clicked() {
                self.cfg.mode = SelectionMode::Windows;
                self.sync_selection();
            }
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let w = ui.available_width() - 110.0;
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("🔍  Поиск…")
                    .desired_width(w)
                    .margin(vec2(10.0, 7.0)),
            );
            if ui.add_sized([100.0, 30.0], egui::Button::new("⟳ Обновить")).clicked() {
                let ctx = ui.ctx().clone();
                self.refresh(&ctx);
            }
        });
        ui.add_space(4.0);

        match self.cfg.mode {
            SelectionMode::Apps => self.render_apps_mode(ui),
            SelectionMode::Windows => self.render_windows_mode(ui),
        }
    }

    fn render_apps_mode(&mut self, ui: &mut egui::Ui) {
        let q = self.search.to_lowercase();
        let mut toggled: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let visible: Vec<_> = self
                .apps
                .iter()
                .filter(|a| {
                    q.is_empty()
                        || a.class.to_lowercase().contains(&q)
                        || a.titles.iter().any(|t| t.to_lowercase().contains(&q))
                })
                .collect();
            if visible.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Нет активных окон или иконок трея").color(MUTED))
                });
            }
            for app in visible {
                let selected = self.cfg.classes.contains(&app.class);
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                let hover = ui.ctx().animate_bool(resp.id.with("h"), resp.hovered());
                let base = if selected { lerp(ROW, ACCENT, 0.18) } else { ROW };
                let p = ui.painter();
                p.rect_filled(rect, 12, lerp(base, ROW_HOVER, hover * 0.7));
                if selected {
                    p.rect_stroke(
                        rect,
                        12,
                        Stroke::new(1.0, ACCENT.gamma_multiply(0.8)),
                        egui::StrokeKind::Inside,
                    );
                }

                // Иконка приложения
                let icon_r =
                    Rect::from_center_size(pos2(rect.left() + 30.0, rect.center().y), vec2(32.0, 32.0));
                match self.icons.get(&app.class).and_then(|t| t.as_ref()) {
                    Some(tex) => {
                        let _ = p.image(
                            tex.id(),
                            icon_r,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    None => {
                        p.rect_filled(icon_r, 8, lerp(ACCENT, ACCENT2, 0.5));
                        let letter =
                            app.class.chars().next().unwrap_or('?').to_uppercase().to_string();
                        p.text(
                            icon_r.center(),
                            Align2::CENTER_CENTER,
                            letter,
                            FontId::proportional(16.0),
                            Color32::WHITE,
                        );
                    }
                }

                let tx = rect.left() + 56.0;
                let max_chars = ((rect.width() - 130.0) / 7.5).max(10.0) as usize;

                // Название класса
                let title_text = if app.in_tray {
                    format!("{} 🔔", short(&app.class, max_chars.saturating_sub(3)))
                } else {
                    short(&app.class, max_chars)
                };
                p.text(
                    pos2(tx, rect.center().y - 9.0),
                    Align2::LEFT_CENTER,
                    title_text,
                    FontId::proportional(15.0),
                    TEXT,
                );

                // Подпись состояния
                let sub = match app.titles.len() {
                    0 if selected && self.status.lock().unwrap().hidden_count > 0 => {
                        "спрятано".to_string()
                    }
                    0 => "не запущено".to_string(),
                    1 => app.titles[0].clone(),
                    n => format!("{} · ещё окон: {}", app.titles[0], n - 1),
                };
                p.text(
                    pos2(tx, rect.center().y + 10.0),
                    Align2::LEFT_CENTER,
                    short(&sub, max_chars),
                    FontId::proportional(12.0),
                    MUTED,
                );

                // Переключатель справа
                let sw = Rect::from_center_size(
                    pos2(rect.right() - 34.0, rect.center().y),
                    vec2(40.0, 22.0),
                );
                switch(ui, sw, selected, resp.id.with("sw"));
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    toggled = Some(app.class.clone());
                }
                ui.add_space(2.0);
            }
        });
        if let Some(c) = toggled {
            if let Some(i) = self.cfg.classes.iter().position(|x| x == &c) {
                self.cfg.classes.remove(i);
            } else {
                self.cfg.classes.push(c);
            }
            self.sync_selection();
        }
    }

    fn render_windows_mode(&mut self, ui: &mut egui::Ui) {
        let q = self.search.to_lowercase();
        let mut toggle_target: Option<WindowTarget> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let visible: Vec<_> = self
                .windows
                .iter()
                .filter(|w| {
                    q.is_empty()
                        || w.title.to_lowercase().contains(&q)
                        || w.class.to_lowercase().contains(&q)
                        || w.pid.map_or(false, |p| p.to_string().contains(&q))
                })
                .collect();

            if visible.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Нет подходящих открытых окон").color(MUTED))
                });
            }

            for win in visible {
                let selected = self.cfg.windows.iter().any(|t| {
                    (t.id != 0 && t.id == win.id)
                        || (t.class == win.class && t.title == win.title)
                });

                let (rect, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
                let hover = ui.ctx().animate_bool(resp.id.with("win_h"), resp.hovered());
                let base = if selected { lerp(ROW, ACCENT, 0.18) } else { ROW };
                let p = ui.painter();
                p.rect_filled(rect, 12, lerp(base, ROW_HOVER, hover * 0.7));
                if selected {
                    p.rect_stroke(
                        rect,
                        12,
                        Stroke::new(1.0, ACCENT.gamma_multiply(0.8)),
                        egui::StrokeKind::Inside,
                    );
                }

                // Иконка
                let icon_r =
                    Rect::from_center_size(pos2(rect.left() + 30.0, rect.center().y), vec2(32.0, 32.0));
                match self.icons.get(&win.class).and_then(|t| t.as_ref()) {
                    Some(tex) => {
                        let _ = p.image(
                            tex.id(),
                            icon_r,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    None => {
                        p.rect_filled(icon_r, 8, lerp(ACCENT, ACCENT2, 0.5));
                        let letter =
                            win.class.chars().next().unwrap_or('?').to_uppercase().to_string();
                        p.text(
                            icon_r.center(),
                            Align2::CENTER_CENTER,
                            letter,
                            FontId::proportional(16.0),
                            Color32::WHITE,
                        );
                    }
                }

                let tx = rect.left() + 56.0;
                let max_chars = ((rect.width() - 130.0) / 7.5).max(10.0) as usize;

                // Заголовок окна
                p.text(
                    pos2(tx, rect.center().y - 9.0),
                    Align2::LEFT_CENTER,
                    short(&win.title, max_chars),
                    FontId::proportional(14.5),
                    TEXT,
                );

                // Инфо: Класс + PID + Живо ли
                let pid_str = win.pid.map_or("".to_string(), |pid| format!(" · PID: {pid}"));
                let status_str = if !win.is_alive {
                    " · (скрыто / неактивно)".to_string()
                } else {
                    "".to_string()
                };
                let sub = format!("{}{}{}", win.class, pid_str, status_str);

                p.text(
                    pos2(tx, rect.center().y + 11.0),
                    Align2::LEFT_CENTER,
                    short(&sub, max_chars),
                    FontId::proportional(12.0),
                    MUTED,
                );

                // Переключатель
                let sw = Rect::from_center_size(
                    pos2(rect.right() - 34.0, rect.center().y),
                    vec2(40.0, 22.0),
                );
                switch(ui, sw, selected, resp.id.with("win_sw"));
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    toggle_target = Some(WindowTarget {
                        id: win.id,
                        title: win.title.clone(),
                        class: win.class.clone(),
                        pid: win.pid,
                    });
                }
                ui.add_space(2.0);
            }
        });

        if let Some(target) = toggle_target {
            if let Some(i) = self.cfg.windows.iter().position(|t| {
                (t.id != 0 && t.id == target.id)
                    || (t.class == target.class && t.title == target.title)
            }) {
                self.cfg.windows.remove(i);
            } else {
                self.cfg.windows.push(target);
            }
            self.sync_selection();
        }
    }

    pub(super) fn hotkey_panel(&mut self, ui: &mut egui::Ui, st: &Status) {
        step_title(ui, 2, "Горячая клавиша", "Сочетание для скрытия и возврата");
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if self.recording {
                let t = ui.input(|i| i.time) as f32;
                let c = lerp(ACCENT, ACCENT2, 0.5 + 0.5 * (t * 4.0).sin());
                ui.label(RichText::new("Нажмите сочетание клавиш…").size(17.0).color(c));
                ui.ctx().request_repaint();
            } else {
                let parts = self.cfg.hotkey.parts();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        ui.label(RichText::new("+").size(18.0).color(MUTED));
                    }
                    keycap(ui, part, st.running);
                }
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let label = if self.recording { "Отмена (Esc)" } else { "✎ Изменить" };
            let btn = ui.add_enabled(!st.running, egui::Button::new(label));
            if btn.clicked() {
                self.recording = !self.recording;
            }
            if st.running {
                ui.label(RichText::new("Остановите, чтобы изменить").size(12.0).color(MUTED));
            }
        });
        ui.add_space(8.0);

        // Опция сокрытия самого Pennix
        let mut hs = self.cfg.hide_self;
        if ui.checkbox(&mut hs, "🔒 Скрывать и само окно Pennix").changed() {
            self.cfg.hide_self = hs;
            self.sync_selection();
        }

        // Опция сокрытия иконок трея
        let mut ht = self.cfg.hide_tray;
        if ui.checkbox(&mut ht, "🔔 Скрывать иконки в системном трее").changed() {
            self.cfg.hide_tray = ht;
            self.sync_selection();
        }
    }

    pub(super) fn run_panel(&mut self, ui: &mut egui::Ui, st: &Status, ctx: &egui::Context) {
        step_title(ui, 3, "Запуск", "Pennix работает в фоне отовсюду");
        ui.add_space(14.0);
        let w = ui.available_width();
        if st.running {
            if big_button(ui, "■  Остановить", RED, Color32::from_rgb(255, 120, 80), w).clicked() {
                self.stop();
            }
            ui.add_space(10.0);
            let text = if st.hidden_count > 0 {
                "👁  Показать всё сейчас"
            } else {
                "🙈  Скрыть всё сейчас"
            };
            if ui.add_sized([w, 34.0], egui::Button::new(text)).clicked() {
                if let Some(e) = &self.engine {
                    e.toggle();
                }
            }
            ui.add_space(8.0);
            let hint = if st.hidden_count > 0 {
                format!(
                    "Объекты спрятаны. Нажмите {} чтобы вернуть.",
                    self.cfg.hotkey.label()
                )
            } else {
                format!(
                    "Нажмите {} в любом месте, чтобы спрятать выбранные окна и трей.",
                    self.cfg.hotkey.label()
                )
            };
            ui.label(RichText::new(hint).color(MUTED));
        } else {
            let (can, count_str) = match self.cfg.mode {
                SelectionMode::Apps => (
                    !self.cfg.classes.is_empty() || self.cfg.hide_self,
                    format!("Выбрано приложений: {}", self.cfg.classes.len()),
                ),
                SelectionMode::Windows => (
                    !self.cfg.windows.is_empty() || self.cfg.hide_self,
                    format!("Выбрано окон: {}", self.cfg.windows.len()),
                ),
            };

            if big_button(ui, "▶  Запустить", ACCENT, ACCENT2, w).clicked() && self.x.is_some() {
                if can {
                    self.recording = false;
                    self.start(ctx);
                } else {
                    self.start_error = match self.cfg.mode {
                        SelectionMode::Apps => Some("Сначала выберите хотя бы одно приложение слева.".into()),
                        SelectionMode::Windows => Some("Сначала выберите хотя бы одно окно слева.".into()),
                    };
                }
            }
            ui.add_space(8.0);
            ui.label(RichText::new(count_str).color(MUTED));
        }
        if let Some(e) = &self.start_error {
            ui.add_space(8.0);
            ui.label(RichText::new(e).color(RED));
        }
    }
}
