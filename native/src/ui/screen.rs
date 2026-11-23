//! Native CenAI screens. No WebView or Compose UI is used in this module.
use crate::ui::model::{AppState, Destination, MessageRole};
use std::cell::Cell;

thread_local! { static LIGHT_MODE: Cell<bool> = const { Cell::new(false) }; }

fn accent() -> egui::Color32 {
    egui::Color32::from_rgb(139, 155, 255)
}

fn text(value: impl Into<String>, size: f32, color: egui::Color32) -> egui::RichText {
    let color = LIGHT_MODE.with(|light| {
        if !light.get() { return color; }
        let spread = color.r().max(color.g()).max(color.b()) - color.r().min(color.g()).min(color.b());
        if spread < 65 && color.r() > 145 {
            egui::Color32::from_rgb(65, 75, 98)
        } else if color == egui::Color32::WHITE {
            egui::Color32::from_rgb(27, 35, 55)
        } else {
            color
        }
    });
    egui::RichText::new(value).size(size).color(color)
}

fn surface<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(24, 29, 49, 232))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(53, 62, 91)))
        .inner_margin(egui::Margin::same(16))
        .corner_radius(egui::CornerRadius::same(20))
        .show(ui, add)
}

fn background(ctx: &egui::Context, time: f32) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let rect = ctx.input(|input| input.screen_rect());
    let light = LIGHT_MODE.with(Cell::f⁄Óù∆≠y