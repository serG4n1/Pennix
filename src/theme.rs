//! Тема оформления и собственные виджеты.

use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, Ui};

pub const BG: Color32 = Color32::from_rgb(13, 14, 28);
pub const CARD: Color32 = Color32::from_rgb(22, 24, 46);
pub const ROW: Color32 = Color32::from_rgb(30, 33, 62);
pub const ROW_HOVER: Color32 = Color32::from_rgb(38, 41, 78);
pub const BORDER: Color32 = Color32::from_rgb(44, 48, 92);
pub const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
pub const ACCENT2: Color32 = Color32::from_rgb(192, 77, 255);
pub const TEXT: Color32 = Color32::from_rgb(234, 234, 255);
pub const MUTED: Color32 = Color32::from_rgb(140, 144, 184);
pub const GREEN: Color32 = Color32::from_rgb(61, 220, 151);
pub const RED: Color32 = Color32::from_rgb(255, 92, 122);
pub const AMBER: Color32 = Color32::from_rgb(255, 190, 92);

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Запасные системные шрифты — для заголовков окон с редкими символами.
    for (i, path) in [
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ]
    .iter()
    .enumerate()
    {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("sys{i}");
            fonts.font_data.insert(name.clone(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            fonts.families.entry(egui::FontFamily::Proportional).or_default().push(name);
        }
    }
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = Color32::from_rgb(16, 17, 34);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.bg_fill = ROW;
    v.widgets.inactive.weak_bg_fill = ROW;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_fill = ROW_HOVER;
    v.widgets.hovered.weak_bg_fill = ROW_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.noninteractive] {
        w.corner_radius = CornerRadius::same(8);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    ctx.set_visuals(v);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(14.0, 7.0);
        s.spacing.scroll.bar_width = 6.0;
    });
}

/// Карточка с рамкой.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(16)
        .inner_margin(18)
        .show(ui, add)
        .inner
}

/// Заголовок шага: кружок с номером + текст.
pub fn step_title(ui: &mut Ui, n: u8, title: &str, sub: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 13.0, ACCENT);
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, n.to_string(), FontId::proportional(14.0), Color32::WHITE);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(egui::RichText::new(title).size(16.0).strong());
            ui.label(egui::RichText::new(sub).size(12.0).color(MUTED));
        });
    });
}

/// Переключатель в стиле iOS.
pub fn switch(ui: &mut Ui, rect: Rect, on: bool, id: egui::Id) {
    let t = ui.ctx().animate_bool(id, on);
    let bg = lerp(Color32::from_rgb(52, 56, 98), ACCENT, t);
    ui.painter().rect_filled(rect, CornerRadius::same((rect.height() / 2.0) as u8), bg);
    let r = rect.height() / 2.0 - 3.0;
    let x = egui::lerp((rect.left() + r + 3.0)..=(rect.right() - r - 3.0), t);
    ui.painter().circle_filled(pos2(x, rect.center().y), r, Color32::WHITE);
}

/// Клавиша-«капа».
pub fn keycap(ui: &mut Ui, text: &str, glow: bool) {
    let font = FontId::monospace(17.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, TEXT);
    let size = vec2(galley.size().x + 26.0, 40.0);
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(r.translate(vec2(0.0, 3.0)), 9, Color32::from_rgb(10, 10, 22));
    p.rect_filled(r, 9, if glow { lerp(ROW, ACCENT, 0.35) } else { ROW });
    p.rect_stroke(r, 9, Stroke::new(1.0, if glow { ACCENT } else { BORDER }), egui::StrokeKind::Inside);
    p.galley(r.center() - galley.size() / 2.0, galley, TEXT);
}

/// Большая градиентная кнопка с подсветкой.
pub fn big_button(ui: &mut Ui, text: &str, c1: Color32, c2: Color32, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 54.0), Sense::click());
    let hover = ui.ctx().animate_bool(resp.id, resp.hovered());
    let p = ui.painter();
    // мягкое свечение
    for i in 1..=6 {
        let a = ((7 - i) as f32 * 6.0 * (0.5 + hover * 0.5)) as u8;
        let c = Color32::from_rgba_unmultiplied(c1.r(), c1.g(), c1.b(), a);
        p.rect_filled(rect.expand(i as f32 * 2.0), 14 + i as u8 * 2, c);
    }
    // горизонтальный градиент из полос
    let steps = 48;
    let clip = p.with_clip_rect(rect);
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let x0 = rect.left() + rect.width() * t0;
        let x1 = rect.left() + rect.width() * (i + 1) as f32 / steps as f32 + 1.0;
        let c = lerp(c1, c2, t0);
        let c = lerp(c, Color32::WHITE, hover * 0.12);
        clip.rect_filled(Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom())), 0, c);
    }
    // скруглённые углы — «вырезаем» цветом фона карточки
    let r = 14.0;
    for (corner, dir) in [
        (rect.left_top(), vec2(1.0, 1.0)),
        (rect.right_top(), vec2(-1.0, 1.0)),
        (rect.left_bottom(), vec2(1.0, -1.0)),
        (rect.right_bottom(), vec2(-1.0, -1.0)),
    ] {
        let center = corner + dir * r;
        let mut pts = vec![corner];
        for k in 0..=8 {
            let ang = std::f32::consts::FRAC_PI_2 * k as f32 / 8.0;
            pts.push(center - vec2(dir.x * ang.cos(), dir.y * ang.sin()) * r);
        }
        p.add(egui::Shape::convex_polygon(pts, CARD, Stroke::NONE));
    }
    p.text(rect.center(), egui::Align2::CENTER_CENTER, text, FontId::proportional(18.0), Color32::WHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
