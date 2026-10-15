//! Native scientific figure panel. All artwork edits use the existing command entry point.

use std::collections::BTreeMap;

use egui::Ui;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::VectorcraftApp;

mod extended;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub columns: usize,
    pub gap_x: f64,
    pub gap_y: f64,
    pub label_size: f64,
    pub label_start: usize,
    pub font: String,
    pub dx: f64,
    pub dy: f64,
    pub width: String,
    pub height: String,
    pub keep_ratio: bool,
    pub scale_strokes: bool,
    pub order: String,
    pub label_style: String,
    pub label_color: String,
    pub label_anchor: String,
    pub align_first: bool,
    pub spacing: f64,
    pub relative_position: bool,
    pub border_weight: f64,
    pub border_color: String,
    pub preset_name: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            columns: 2,
            gap_x: 12.0,
            gap_y: 12.0,
            label_size: 14.0,
            label_start: 1,
            font: "Arial".into(),
            dx: 4.0,
            dy: 4.0,
            width: String::new(),
            height: String::new(),
            keep_ratio: true,
            scale_strokes: false,
            order: "rows".into(),
            label_style: "A".into(),
            label_color: "#000000".into(),
            label_anchor: "top-left".into(),
            align_first: false,
            spacing: 12.0,
            relative_position: false,
            border_weight: 1.0,
            border_color: "#000000".into(),
            preset_name: "默认预设".into(),
        }
    }
}

#[derive(Clone, Debug)]
struct BoxInfo {
    id: u64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Clone, Debug)]
pub struct PositionClipboard {
    boxes: Vec<BoxInfo>,
    relative: bool,
}

fn bounds(node: &Value) -> Result<BoxInfo, String> {
    let b = node.get("bounds").ok_or("对象没有可用边界")?;
    let finite = |key: &str| b.get(key).and_then(Value::as_f64).filter(|n| n.is_finite()).ok_or_else(|| format!("对象边界 {key} 无效"));
    let result = BoxInfo {
        id: node.get("id").and_then(Value::as_u64).ok_or("对象编号无效")?,
        x: finite("x")?,
        y: finite("y")?,
        w: finite("width")?,
        h: finite("height")?,
    };
    if result.w <= 1e-7 || result.h <= 1e-7 {
        return Err("请排除空对象、零宽或零高对象".into());
    }
    Ok(result)
}

fn selection(app: &mut VectorcraftApp) -> Result<(Vec<BoxInfo>, BTreeMap<u64, Value>), String> {
    let doc = app.run("document.inspect", json!({}))?;
    if !doc.get("isolation").unwrap_or(&Value::Null).is_null() {
        return Err("请先退出隔离编辑".into());
    }
    let ids: Vec<u64> = doc.get("selection").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).collect();
    if ids.is_empty() || ids.len() > 200 {
        return Err("请选择 1–200 个对象".into());
    }
    let mut nodes = BTreeMap::new();
    let mut stack: Vec<(&Value, bool, bool)> = doc.get("layers").and_then(Value::as_array).into_iter().flatten().map(|n| (n, false, false)).collect();
    while let Some((node, blocked_parent, selected_parent)) = stack.pop() {
        let id = node.get("id").and_then(Value::as_u64).ok_or("对象编号无效")?;
        let blocked = blocked_parent
            || node.get("locked").and_then(Value::as_bool).unwrap_or(false)
            || node.get("template").and_then(Value::as_bool).unwrap_or(false)
            || !node.get("visible").and_then(Value::as_bool).unwrap_or(true);
        let selected = ids.contains(&id);
        if selected && (blocked || selected_parent || node.get("kind").and_then(Value::as_str) == Some("Layer")) {
            return Err("不能选择隐藏、锁定、图层或同时选择组合与其内部对象".into());
        }
        let mut summary = node.clone();
        summary["sciBlocked"] = json!(blocked);
        nodes.insert(id, summary);
        for child in node.get("children").and_then(Value::as_array).into_iter().flatten() {
            stack.push((child, blocked, selected_parent || selected));
        }
    }
    let boxes = ids.iter().map(|id| nodes.get(id).ok_or_else(|| "所选对象不存在".to_string()).and_then(bounds)).collect::<Result<Vec<_>, _>>()?;
    Ok((boxes, nodes))
}

fn reading_order(boxes: &mut [BoxInfo]) {
    boxes.sort_by(|a, b| (a.y + a.h / 2.0).total_cmp(&(b.y + b.h / 2.0)).then(a.x.total_cmp(&b.x)));
    let mut first = 0;
    while first < boxes.len() {
        let Some(reference) = boxes.get(first).cloned() else { break };
        let mut end = first + 1;
        while let Some(next) = boxes.get(end) {
            if ((next.y + next.h / 2.0) - (reference.y + reference.h / 2.0)).abs() > 0.45 * next.h.min(reference.h) {
                break;
            }
            end += 1;
        }
        if let Some(row) = boxes.get_mut(first..end) {
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
        }
        first = end;
    }
}

fn transaction(
    app: &mut VectorcraftApp,
    label: &str,
    ids: Vec<u64>,
    work: impl FnOnce(&mut VectorcraftApp) -> Result<(), String>,
) -> Result<(), String> {
    app.run("edit.undoGroup.begin", json!({"label": label}))?;
    let outcome = work(app).and_then(|()| app.run("select.set", json!({"ids": ids})).map(|_| ()));
    match outcome {
        Ok(()) => app.run("edit.undoGroup.end", json!({})).map(|_| ()),
        Err(error) => match app.run("edit.undoGroup.end", json!({"cancel": true})) {
            Ok(_) => Err(error),
            Err(rollback) => Err(format!("{error}；回滚失败：{rollback}")),
        },
    }
}

fn move_object(app: &mut VectorcraftApp, b: &BoxInfo, x: f64, y: f64) -> Result<(), String> {
    app.run("object.transform", json!({"ids": [b.id], "matrix": [1, 0, 0, 1, x-b.x, y-b.y], "patterns": true})).map(|_| ())
}

pub(crate) fn grid(app: &mut VectorcraftApp, settings: &Settings) -> Result<(), String> {
    extended::layout(app, settings, "grid", "")
}

fn letters(mut index: usize) -> String {
    let mut text = vec![];
    while index > 0 {
        index -= 1;
        text.push(char::from(b'A' + (index % 26) as u8));
        index /= 26;
    }
    text.iter().rev().collect()
}

fn label_text(index: usize, style: &str) -> Result<String, String> {
    let base = match style {
        "A" | "A)" => letters(index),
        "a" | "a)" => letters(index).to_lowercase(),
        "1" | "1)" => index.to_string(),
        _ => return Err("未知编号格式".into()),
    };
    Ok(if style.ends_with(')') { format!("{base})") } else { base })
}

fn attribute<'a>(detail: &'a Value, key: &str) -> Option<&'a str> {
    let data = detail.pointer("/attrs/data")?;
    if let Some(value) = data.get(key).and_then(Value::as_str) {
        return Some(value);
    }
    // Native Attributes stores ordered key/value pairs, rather than a JSON map.
    data.as_array()?.iter().find_map(|pair| (pair.get(0).and_then(Value::as_str) == Some(key)).then(|| pair.get(1).and_then(Value::as_str)).flatten())
}

pub(crate) fn labels(app: &mut VectorcraftApp, settings: &Settings) -> Result<(), String> {
    let (mut boxes, nodes) = selection(app)?;
    if !(1..=9999).contains(&settings.label_start)
        || settings.label_start.saturating_add(boxes.len()) > 10000
        || !settings.label_size.is_finite()
        || !(1.0..=300.0).contains(&settings.label_size)
        || settings.font.trim().is_empty()
        || settings.font.len() > 120
        || [settings.dx, settings.dy].iter().any(|n| !n.is_finite() || n.abs() > 100_000.0)
    {
        return Err("请检查编号、字号、字体和偏移".into());
    }
    extended::ordered(&mut boxes, &settings.order)?;
    extended::valid_color(&settings.label_color)?;
    label_text(settings.label_start, &settings.label_style)?;
    if !["top-left", "top-right", "bottom-left", "bottom-right"].contains(&settings.label_anchor.as_str()) {
        return Err("未知标签锚点".into());
    }
    if boxes.iter().any(|b| nodes.get(&b.id).and_then(|n| n.get("name")).and_then(Value::as_str).unwrap_or("").starts_with("[SCI ")) {
        return Err("请只选择原始对象，不要同时选择科研标签或边框".into());
    }
    let mut existing = BTreeMap::new();
    for (id, node) in &nodes {
        if node.get("name").and_then(Value::as_str).unwrap_or("").starts_with("[SCI label]") {
            let detail = app.run("document.node", json!({"id": id}))?;
            if let Some(target) = attribute(&detail, "sci-target").and_then(|s| s.parse::<u64>().ok())
                && boxes.iter().any(|b| b.id == target)
            {
                if node.get("sciBlocked").and_then(Value::as_bool).unwrap_or(false) {
                    return Err("请先解锁并显示已有科研标签及其父组合".into());
                }
                if existing.insert(target, *id).is_some() {
                    return Err("同一对象存在多个关联标签，请先整理重复标签".into());
                }
            }
        }
    }
    transaction(app, "添加或更新子图标签", boxes.iter().map(|b| b.id).collect(), |app| {
        for (i, b) in boxes.iter().enumerate() {
            let text = label_text(settings.label_start + i, &settings.label_style)?;
            let id = if let Some(id) = existing.get(&b.id) {
                app.run("text.setText", json!({"ids": [id], "text": text}))?;
                *id
            } else {
                let created = app.run(
                    "text.create",
                    json!({"x": b.x, "y": b.y, "text": text, "size": settings.label_size, "font": settings.font, "color": settings.label_color}),
                )?;
                let id = created.get("id").and_then(Value::as_u64).ok_or("无法读取新标签编号")?;
                app.run(
                    "object.setProps",
                    json!({"ids": [id], "name": format!("[SCI label] {}", b.id), "data": {"sci-target": b.id.to_string(), "sci-role": "label"}}),
                )?;
                id
            };
            app.run("text.setStyle", json!({"ids": [id], "font": settings.font, "size": settings.label_size, "fill": settings.label_color}))?;
            let current = bounds(&app.run("document.node", json!({"id": id, "summary": true}))?)?;
            let x = if settings.label_anchor.ends_with("right") { b.x + b.w - current.w } else { b.x };
            let y = if settings.label_anchor.starts_with("bottom") { b.y + b.h - current.h } else { b.y };
            move_object(app, &current, x + settings.dx, y + settings.dy)?;
        }
        Ok(())
    })
}

/// The panel and automation share the same entry point; no background service is involved.
pub fn command(app: &mut VectorcraftApp, params: &Value) -> Result<Value, String> {
    let operation = params.get("operation").and_then(Value::as_str).unwrap_or("inspect");
    let mut value = serde_json::to_value(&app.ui.scientific).map_err(|e| e.to_string())?;
    if let Some(patch) = params.get("settings") {
        let patch = patch.as_object().ok_or("settings 应为参数对象")?;
        let base = value.as_object_mut().ok_or("无法读取科研参数")?;
        for (key, value) in patch {
            base.insert(key.clone(), value.clone());
        }
    }
    let mut settings: Settings = serde_json::from_value(value).map_err(|e| format!("科研参数无效：{e}"))?;
    let axis = params.get("axis").and_then(Value::as_str).unwrap_or("");
    match operation {
        "grid" => grid(app, &settings)?,
        "resize" | "align" | "spacing" | "swap" | "paste_position" => extended::layout(app, &settings, operation, axis)?,
        "copy_position" => extended::copy(app, &settings)?,
        "labels" => labels(app, &settings)?,
        "borders" => extended::borders(app, &settings)?,
        "preset_save" | "preset_load" | "preset_delete" => {
            let name = settings.preset_name.trim().to_string();
            if name.is_empty() || name.len() > 120 {
                return Err("请输入不超过 120 字节的预设名称".into());
            }
            match operation {
                "preset_save" => {
                    if app.ui.scientific_presets.len() >= 32 && !app.ui.scientific_presets.contains_key(&name) {
                        return Err("最多保存 32 个预设".into());
                    }
                    settings.preset_name = name.clone();
                    app.ui.scientific_presets.insert(name, settings.clone());
                }
                "preset_load" => settings = app.ui.scientific_presets.get(&name).cloned().ok_or("预设不存在")?,
                _ => {
                    app.ui.scientific_presets.remove(&name).ok_or("预设不存在")?;
                }
            }
        }
        "configure" | "inspect" => (),
        _ => return Err("未知科研组图操作".into()),
    }
    if operation != "inspect" {
        app.ui.scientific = settings;
    }
    Ok(
        json!({"operation":operation,"settings":app.ui.scientific,"selected":app.session.active().map_or(0,|d|d.selection.len()),"clipboard":app.ui.scientific_clipboard.is_some(),"presets":app.ui.scientific_presets.keys().collect::<Vec<_>>()}),
    )
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    extended::show(app, ui);
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    fn fixture() -> (VectorcraftApp, Vec<u64>) {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 612, "height": 792})).unwrap();
        let mut ids = vec![];
        for (x, y, w, h) in [(10, 20, 30, 20), (95, 22, 40, 20), (25, 110, 25, 35)] {
            let result = app.run("shape.rectangle", json!({"x":x,"y":y,"width":w,"height":h})).unwrap();
            ids.push(result["id"].as_u64().unwrap());
        }
        app.run("select.set", json!({"ids":ids})).unwrap();
        (app, ids)
    }

    #[test]
    fn native_grid_keeps_ids_and_selection_and_undoes_in_one_step() {
        let (mut app, ids) = fixture();
        let before = app.run("document.inspect", json!({})).unwrap();
        let history = before["history"].as_array().unwrap().len();
        app.run("ui.scientific", json!({"operation":"grid", "settings":{"columns":2,"gap_x":10,"gap_y":15}})).unwrap();
        let after = app.run("document.inspect", json!({})).unwrap();
        assert_eq!(after["selection"], json!(ids));
        assert_eq!(after["history"].as_array().unwrap().len(), history + 1);
        let positions: Vec<_> = ids.iter().map(|id| app.run("document.node", json!({"id":id,"summary":true})).unwrap()["bounds"].clone()).collect();
        assert_eq!(
            (positions[0]["x"].as_f64().unwrap(), positions[1]["x"].as_f64().unwrap(), positions[2]["y"].as_f64().unwrap()),
            (10.0, 50.0, 55.0)
        );
        app.run("edit.undo", json!({})).unwrap();
        let undone = app.run("document.inspect", json!({})).unwrap();
        assert_eq!(undone["layers"], before["layers"]);
    }

    #[test]
    fn native_labels_update_without_duplicates_and_reject_invalid_settings_without_edits() {
        let (mut app, ids) = fixture();
        app.run("ui.scientific", json!({"operation":"labels"})).unwrap();
        let once = app.run("document.inspect", json!({})).unwrap();
        app.run("ui.scientific", json!({"operation":"labels", "settings":{"label_start":3}})).unwrap();
        let twice = app.run("document.inspect", json!({})).unwrap();
        assert_eq!(once["objects"], twice["objects"]);
        assert_eq!(twice["selection"], json!(ids));
        assert!(app.run("ui.scientific", json!({"operation":"grid","settings":{"columns":0}})).is_err());
        assert_eq!(app.run("document.inspect", json!({})).unwrap()["layers"], twice["layers"]);
        assert_eq!(letters(26), "Z");
        assert_eq!(letters(27), "AA");
    }

    #[test]
    fn explicit_failure_rolls_back_partial_native_transaction() {
        let (mut app, ids) = fixture();
        let before = app.run("document.inspect", json!({})).unwrap();
        let result = transaction(&mut app, "intentional rollback", ids.clone(), |app| {
            app.run("object.transform", json!({"ids": ids, "matrix": [1,0,0,1,123,50]}))?;
            Err("deliberate test refusal".into())
        });
        assert!(result.is_err());
        let after = app.run("document.inspect", json!({})).unwrap();
        assert_eq!(before["layers"], after["layers"]);
        assert_eq!(before["history"], after["history"]);
        assert_eq!(before["selection"], after["selection"]);
    }
}
