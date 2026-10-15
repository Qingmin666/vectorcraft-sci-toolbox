//! Parameter-only colour selection; never applies paint to the current artwork.
use egui::Ui;
use serde_json::json;

use crate::VectorcraftApp;
use crate::panels::c32;
use crate::panels::color::parse_hex;

pub fn parameter_color(app: &VectorcraftApp, target: &str) -> Result<vectorcraft_color::Color, String> {
    let value = match target {
        "labels" => &app.ui.scientific.label_color,
        "borders" => &app.ui.scientific.border_color,
        "scale" => &app.ui.scientific.scale.color,
        "scale_text" => &app.ui.scientific.scale.font_color,
        "zoom" => &app.ui.scientific.zoom.color,
        other => {
            let index = other.strip_prefix("zoom-region-").and_then(|v| v.parse::<usize>().ok()).ok_or("未知的科研颜色目标")?;
            let r = app.ui.scientific.zoom.regions.get(index).ok_or("ROI 已不存在")?;
            if r.color.is_empty() { &app.ui.scientific.zoom.color } else { &r.color }
        }
    };
    parse_hex(value).ok_or("参数颜色无效".into())
}

pub fn set_parameter(app: &mut VectorcraftApp, target: &str, value: &str) -> Result<(), String> {
    let color = parse_hex(value).ok_or("科研参数颜色无效")?.to_hex().to_uppercase();
    match target {
        "labels" => app.ui.scientific.label_color = color.clone(),
        "borders" => app.ui.scientific.border_color = color.clone(),
        "scale" => app.ui.scientific.scale.color = color.clone(),
        "scale_text" => app.ui.scientific.scale.font_color = color.clone(),
        "zoom" => app.ui.scientific.zoom.color = color.clone(),
        other => {
            let index = other.strip_prefix("zoom-region-").and_then(|v| v.parse::<usize>().ok()).ok_or("未知的科研颜色目标")?;
            let r = app.ui.scientific.zoom.regions.get_mut(index).ok_or("ROI 已不存在")?;
            r.color = color.clone();
        }
    }
    app.ui.scientific_recent_colors.retain(|s| !s.eq_ignore_ascii_case(&color));
    app.ui.scientific_recent_colors.insert(0, color);
    app.ui.scientific_recent_colors.truncate(12);
    Ok(())
}

pub fn field(app: &mut VectorcraftApp, ui: &mut Ui, value: &mut String, target: &str) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let response = ui.add(egui::TextEdit::singleline(value).desired_width(82.0));
            if response.lost_focus() && parse_hex(value).is_some() {
                if let Some(color) = parse_hex(value) {
                    *value = color.to_hex().to_uppercase();
                }
                if let Err(e) = app.run("ui.scientific", json!({"operation":"set_color","target":target,"color":value})) {
                    app.status(e);
                }
            }
            if let Some(color) = parse_hex(value) {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, c32(&color));
            }
            if ui.button("取色…").on_hover_text("使用原生取色器，只更新此参数").clicked()
                && let Err(e) = app.run("ui.colorPicker", json!({"scientificTarget":target,"color":value}))
            {
                app.status(e);
            }
        });
        egui::ComboBox::from_id_salt(("sci-colors", target)).selected_text("最近颜色 / 文档色板").show_ui(ui, |ui| {
            let mut chosen = None;
            ui.label("最近颜色");
            for color in app.ui.scientific_recent_colors.iter().take(12) {
                if color_row(ui, color, color) {
                    chosen = Some(color.clone());
                }
            }
            ui.separator();
            ui.label("文档纯色（RGB 快照）");
            if let Some(st) = app.session.active() {
                for swatch in st.doc.swatches_iter().filter(|w| !w.spot && !w.is_reserved()).take(256) {
                    if let Some(color) = swatch.paint.color() {
                        let hex = color.to_hex().to_uppercase();
                        if color_row(ui, &format!("{}  {}", swatch.name, hex), &hex) {
                            chosen = Some(hex);
                        }
                    }
                }
            }
            if let Some(color) = chosen {
                *value = color.clone();
                if let Err(e) = app.run("ui.scientific", json!({"operation":"set_color","target":target,"color":color})) {
                    app.status(e);
                }
            }
        });
    });
}

fn color_row(ui: &mut Ui, name: &str, value: &str) -> bool {
    ui.horizontal(|ui| {
        if let Some(color) = parse_hex(value) {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, c32(&color));
        }
        ui.selectable_label(false, name).clicked()
    })
    .inner
}
