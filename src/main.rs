//! Pennix для Linux — скрытие окон и иконок системного трея по горячей клавише (X11).

mod app;
mod config;
mod engine;
mod theme;
mod tray;
mod x11;
mod x11_ops;

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/pennix.png")).ok();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Pennix для Linux")
        .with_app_id("pennix")
        .with_inner_size([900.0, 620.0])
        .with_min_inner_size([760.0, 520.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Pennix для Linux",
        options,
        Box::new(|cc| Ok(Box::new(app::PennixApp::new(cc)))),
    )
}
