//! Additional native layout operations and the complete scientific panel.
use super::*;

pub(super) fn valid_color(color: &str) -> Result<(), String> {
    if color.len() == 7 && color.starts_with('#') && color.bytes().skip(1).all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("颜色应为 #RRGGBB 六位十六进制色值".into())
    }
}

pub(super) fn ordered(boxes: &mut [BoxInfo], order: &str) -> Result<(), String> {
    match order {
        "rows" => reading_order(boxes),
        "horizontal" => boxes.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))),
        "vertical" => boxes.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x))),
        "selection" => (),
        _ => return Err("未知排序方式".into()),
    }
    Ok(())
}

fn dimension(text: &str) -> Result<Option<f64>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    text.trim().parse::<f64>().ok().filter(|n| n.is_finite() && (0.01..=100_000.0).contains(n)).map(Some).ok_or("尺寸应为 0.01–100000 pt".into())
}

fn resized(boxes: &[BoxInfo], settings: &Settings, required: bool) -> Result<Vec<BoxInfo>, String> {
    let width = dimension(&settings.width)?;
    let height = dimension(&settings.height)?;
    if width.is_none() && height.is_none() {
        return if required { Err("至少填写宽度或高度".into()) } else { Ok(boxes.to_vec()) };
    }
    if settings.keep_ratio && width.is_some() && height.is_some() {
        return Err("保持比例时只填写宽或高其中一项".into());
    }
    Ok(boxes
        .iter()
        .map(|b| {
            let mut result = b.clone();
            result.w = width.unwrap_or(b.w);
            result.h = height.unwrap_or(b.h);
            if settings.keep_ratio {
                if width.is_some() {
                    result.h = b.h * result.w / b.w;
                } else {
                    result.w = b.w * result.h / b.h;
                }
            }
            result
        })
        .collect())
}

fn transform(app: &mut VectorcraftApp, source: &BoxInfo, target: &BoxInfo, strokes: bool) -> Result<(), String> {
    let sx = target.w / source.w;
    let sy = target.h / source.h;
    app.run(
        "object.transform",
        json!({"ids":[source.id], "matrix":[sx,0,0,sy,target.x-sx*source.x,target.y-sy*source.y], "strokes":strokes,"corners":false,"patterns":true}),
    )
    .map(|_| ())
}

pub(super) fn copy(app: &mut VectorcraftApp, settings: &Settings) -> Result<(), String> {
    let (mut boxes, _) = selection(app)?;
    super::advanced::order(app, &mut boxes, settings)?;
    super::advanced::fractions(&settings.position_anchor)?;
    let artboard = settings.artboard_reference || settings.relative_position && boxes.len() == 1;
    let origins = if artboard { super::advanced::origins(app, &boxes, &settings.position_anchor)? } else { Vec::new() };
    app.ui.scientific_clipboard =
        Some(PositionClipboard { boxes, relative: settings.relative_position, anchor: settings.position_anchor.clone(), artboard, origins });
    Ok(())
}

pub(super) fn layout(app: &mut VectorcraftApp, settings: &Settings, operation: &str, axis: &str) -> Result<(), String> {
    let (boxes, _) = selection(app)?;
    let mut plan = boxes.clone();
    match operation {
        "resize" => plan = resized(&boxes, settings, true)?,
        "grid" => {
            if !(1..=100).contains(&settings.columns)
                || [settings.gap_x, settings.gap_y].iter().any(|n| !n.is_finite() || !(0.0..=100_000.0).contains(n))
            {
                return Err("请检查列数和行列间距".into());
            }
            if !["original", "auto", "custom"].contains(&settings.size_mode.as_str()) {
                return Err("未知的网格尺寸模式".into());
            }
            plan = if settings.size_mode != "custom" { boxes.clone() } else { resized(&boxes, settings, false)? };
            super::advanced::order(app, &mut plan, settings)?;
            if settings.auto_layout || settings.align_edges || settings.size_mode == "auto" {
                plan = super::advanced::grid(&plan, settings, &boxes)?;
            } else {
                let cols = settings.columns.min(plan.len());
                let mut widths = vec![0.0_f64; cols];
                let mut heights = vec![0.0_f64; plan.len().div_ceil(cols)];
                for (i, b) in plan.iter().enumerate() {
                    if let Some(w) = widths.get_mut(i % cols) {
                        *w = w.max(b.w);
                    }
                    if let Some(h) = heights.get_mut(i / cols) {
                        *h = h.max(b.h);
                    }
                }
                let x0 = plan.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
                let y0 = plan.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
                for (i, b) in plan.iter_mut().enumerate() {
                    b.x = x0 + widths.iter().take(i % cols).sum::<f64>() + settings.gap_x * (i % cols) as f64;
                    b.y = y0 + heights.iter().take(i / cols).sum::<f64>() + settings.gap_y * (i / cols) as f64;
                }
            }
        }
        "align" => {
            let first = boxes.first().ok_or("没有所选对象")?;
            let reference = if settings.align_first {
                first.clone()
            } else {
                let x = boxes.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
                let y = boxes.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
                let right = boxes.iter().map(|b| b.x + b.w).fold(f64::NEG_INFINITY, f64::max);
                let bottom = boxes.iter().map(|b| b.y + b.h).fold(f64::NEG_INFINITY, f64::max);
                BoxInfo { id: 0, x, y, w: right - x, h: bottom - y }
            };
            for b in &mut plan {
                match axis {
                    "left" => b.x = reference.x,
                    "center" => b.x = reference.x + (reference.w - b.w) / 2.0,
                    "right" => b.x = reference.x + reference.w - b.w,
                    "top" => b.y = reference.y,
                    "middle" => b.y = reference.y + (reference.h - b.h) / 2.0,
                    "both" => {
                        b.x = reference.x + (reference.w - b.w) / 2.;
                        b.y = reference.y + (reference.h - b.h) / 2.;
                    }
                    "bottom" => b.y = reference.y + reference.h - b.h,
                    _ => return Err("未知对齐方向".into()),
                }
            }
        }
        "spacing" => {
            if boxes.len() < 2 || !settings.spacing.is_finite() || !(0.0..=100_000.0).contains(&settings.spacing) {
                return Err("等间距需要至少两个对象和有效的非负间距".into());
            }
            if !["horizontal", "vertical"].contains(&axis) {
                return Err("未知分布方向".into());
            }
            ordered(&mut plan, axis)?;
            let first = plan.first().ok_or("没有所选对象")?;
            let mut cursor = if axis == "horizontal" { first.x } else { first.y };
            for b in &mut plan {
                if axis == "horizontal" {
                    b.x = cursor;
                    cursor += b.w + settings.spacing;
                } else {
                    b.y = cursor;
                    cursor += b.h + settings.spacing;
                }
            }
            if settings.spacing_from_end {
                let last = plan.last().ok_or("没有间距目标")?;
                let original = boxes.iter().find(|b| b.id == last.id).ok_or("缺少间距参考")?;
                let shift = if axis == "horizontal" { original.x - last.x } else { original.y - last.y };
                for b in &mut plan {
                    if axis == "horizontal" {
                        b.x += shift;
                    } else {
                        b.y += shift;
                    }
                }
            }
        }
        "swap" | "distribute" | "spacing_auto" => plan = super::advanced::extra_plan(&boxes, settings, operation, axis)?,

        "paste_position" => {
            let copied = app.ui.scientific_clipboard.clone().ok_or("请先复制位置")?;
            if copied.boxes.len() != boxes.len() && copied.boxes.len() != 1 && !settings.allow_position_mismatch {
                return Err(format!("请选择 {} 个目标对象，或启用循环应用位置", copied.boxes.len()));
            }
            if [settings.position_dx, settings.position_dy].iter().any(|v| !v.is_finite() || v.abs() > 100000.) {
                return Err("位置覆盖参数无效".into());
            }
            super::advanced::order(app, &mut plan, settings)?;
            let (fx, fy) = super::advanced::fractions(&copied.anchor)?;
            let first = plan.first().ok_or("没有目标对象")?;
            let target_reference = [first.x + fx * first.w, first.y + fy * first.h];
            let source = copied.boxes.first().ok_or("位置剪贴板为空")?;
            let source_reference = [source.x + fx * source.w, source.y + fy * source.h];
            let origins = if copied.artboard { super::advanced::origins(app, &plan, &copied.anchor)? } else { Vec::new() };
            for (i, b) in plan.iter_mut().enumerate() {
                let relative = copied.relative && !copied.artboard;
                if relative && i == 0 {
                    continue;
                }
                let index = if relative { 1 + (i - 1) % copied.boxes.len().saturating_sub(1).max(1) } else { i % copied.boxes.len() };
                let saved = copied.boxes.get(index).ok_or("位置模式不完整")?;
                let mut point = [saved.x + fx * saved.w, saved.y + fy * saved.h];
                if relative {
                    let sign = if settings.position_reverse { -1. } else { 1. };
                    let delta = if settings.position_override {
                        [settings.position_dx, settings.position_dy]
                    } else {
                        [point[0] - source_reference[0], point[1] - source_reference[1]]
                    };
                    point = [target_reference[0] + sign * delta[0], target_reference[1] + sign * delta[1]];
                } else if copied.artboard {
                    let old = copied.origins.get(index).ok_or("缺少源画板")?;
                    let new = origins.get(i).ok_or("缺少目标画板")?;
                    point = if settings.position_override {
                        [new[0] + settings.position_dx, new[1] + settings.position_dy]
                    } else {
                        [point[0] - old[0] + new[0], point[1] - old[1] + new[1]]
                    };
                } else if settings.position_override {
                    point = [settings.position_dx, settings.position_dy];
                }
                b.x = point[0] - fx * b.w;
                b.y = point[1] - fy * b.h;
            }
        }

        _ => return Err("未知排版操作".into()),
    }
    let before: BTreeMap<u64, &BoxInfo> = boxes.iter().map(|b| (b.id, b)).collect();
    let changed: Vec<_> = plan
        .iter()
        .filter(|b| before.get(&b.id).is_some_and(|s| [b.x - s.x, b.y - s.y, b.w - s.w, b.h - s.h].iter().any(|d| d.abs() > 1e-8)))
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    let label = match operation {
        "grid" => "科研网格排图",
        "resize" => "统一对象尺寸",
        "align" => "科研对齐",
        "spacing" => "科研等间距",
        "swap" => "交换对象位置",
        _ => "粘贴对象位置",
    };
    transaction(app, label, boxes.iter().map(|b| b.id).collect(), |app| {
        for b in &changed {
            let source = before.get(&b.id).ok_or("原对象不存在")?;
            transform(app, source, b, settings.scale_strokes)?;
        }
        Ok(())
    })
}

pub(super) fn borders(app: &mut VectorcraftApp, settings: &Settings) -> Result<(), String> {
    valid_color(&settings.border_color)?;
    if !settings.border_weight.is_finite()
        || !(0.01..=100.0).contains(&settings.border_weight)
        || !settings.border_dash.is_finite()
        || !(0.0..=10000.).contains(&settings.border_dash)
    {
        return Err("边框线宽应为 0.01–100 pt".into());
    }
    let (mut boxes, nodes) = selection(app)?;
    for b in &mut boxes {
        let detail = app.run("document.node", json!({"id":b.id}))?;
        if matches!(attribute(&detail, "sci-role"), Some("border-wrapper" | "scale-wrapper")) {
            let target = attribute(&detail, "sci-target").and_then(|s| s.parse::<u64>().ok()).ok_or("边框组合的关联已损坏")?;
            *b = bounds(nodes.get(&target).ok_or("原图已被移除")?)?;
        }
    }
    if boxes.iter().any(|b| nodes.get(&b.id).and_then(|n| n.get("name")).and_then(Value::as_str).unwrap_or("").starts_with("[SCI ")) {
        return Err("请只选择原始对象".into());
    }
    let mut existing = BTreeMap::new();
    for (id, node) in &nodes {
        if node.get("name").and_then(Value::as_str).unwrap_or("").starts_with("[SCI border]") {
            let detail = app.run("document.node", json!({"id":id}))?;
            if let Some(target) = attribute(&detail, "sci-target").and_then(|s| s.parse::<u64>().ok())
                && boxes.iter().any(|b| b.id == target)
            {
                if node.get("sciBlocked").and_then(Value::as_bool).unwrap_or(false) {
                    return Err("请先解锁并显示关联边框".into());
                }
                if existing.insert(target, *id).is_some() {
                    return Err("同一对象存在重复关联边框".into());
                }
            }
        }
    }
    transaction(app, "添加或更新图片边框", boxes.iter().map(|b| b.id).collect(), |app| {
        for b in &boxes {
            let id = if let Some(id) = existing.get(&b.id) {
                let current = bounds(&app.run("document.node", json!({"id":id,"summary":true}))?)?;
                let target = BoxInfo { id: *id, ..b.clone() };
                transform(app, &current, &target, false)?;
                *id
            } else {
                let made = app.run("shape.rectangle", json!({"x":b.x,"y":b.y,"width":b.w,"height":b.h}))?;
                let id = made.get("id").and_then(Value::as_u64).ok_or("新边框编号无效")?;
                app.run(
                    "object.setProps",
                    json!({"ids":[id],"name":format!("[SCI border] {}",b.id),"data":{"sci-target":b.id.to_string(),"sci-role":"border"}}),
                )?;
                id
            };
            app.run("paint.setFill", json!({"ids":[id],"none":true}))?;
            app.run("paint.setStroke", json!({"ids":[id],"color":settings.border_color}))?;
            app.run("stroke.set", json!({"ids":[id],"weight":settings.border_weight,"align":"inside","dash":if settings.border_dash>0. {json!([settings.border_weight*2.,settings.border_dash])}else{json!(null)}}))?;
            if settings.border_group {
                let parent = app.session.active().and_then(|st| st.doc.parent_of(vectorcraft_doc::NodeId(b.id)));
                let grouped = if let Some(parent) = parent {
                    let d = app.run("document.node", json!({"id":parent.0}))?;
                    matches!(attribute(&d, "sci-role"), Some("border-wrapper" | "scale-wrapper"))
                } else {
                    false
                };
                if grouped && let Some(parent) = parent {
                    app.run("layer.move", json!({"ids":[id],"target":parent.0,"place":"inside"}))?;
                }
                if !grouped {
                    app.run("select.set", json!({"ids":[b.id,id]}))?;
                    let g = app.run("object.group", json!({}))?;
                    app.run("object.setProps",json!({"ids":[g.get("id").ok_or("缺少边框组合")?],"name":"科研边框组","data":{"sci-role":"border-wrapper","sci-target":b.id.to_string()}}))?;
                }
            }
        }
        Ok(())
    })
}

fn action(app: &mut VectorcraftApp, settings: &Settings, operation: &str, axis: &str) {
    app.ui.status = match app.run("ui.scientific", json!({"operation":operation,"axis":axis,"settings":settings})) {
        Ok(_) => {
            if operation.starts_with("preset_") {
                "预设操作完成".into()
            } else if operation == "copy_position" {
                "位置已复制".into()
            } else {
                "操作完成，可一次撤销".into()
            }
        }
        Err(e) => e,
    };
}

fn choice(ui: &mut Ui, id: &str, current: &mut String, items: &[(&str, &str)]) {
    let label = items.iter().find(|(value, _)| *value == current.as_str()).map_or(current.as_str(), |(_, label)| *label).to_string();
    egui::ComboBox::from_id_salt(id).selected_text(label).width(130.0).show_ui(ui, |ui| {
        for (value, label) in items {
            ui.selectable_value(current, (*value).into(), *label);
        }
    });
}

pub(super) fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let mut s = app.ui.scientific.clone();
    let count = app.session.active().map_or(0, |d| d.selection.len());
    ui.heading("科研组图");
    ui.small("原生内嵌版 · v0.2.0-rc.2");
    ui.label(format!("当前选择：{count} 个对象 · 单位 pt"));
    ui.horizontal(|ui| {
        if ui.button("撤销").clicked()
            && let Err(e) = app.run("edit.undo", json!({}))
        {
            app.ui.status = e;
        }
        if ui.button("重做").clicked()
            && let Err(e) = app.run("edit.redo", json!({}))
        {
            app.ui.status = e;
        }
    });
    choice(
        ui,
        "sci-order",
        &mut s.order,
        &[
            ("rows", "按行排序"),
            ("horizontal", "从左到右"),
            ("vertical", "从上到下"),
            ("selection", "接口选择顺序"),
            ("stacking", "图层顺序（自下而上）"),
        ],
    );
    ui.separator();
    egui::CollapsingHeader::new("排图与尺寸").default_open(true).show(ui, |ui| {
        egui::Grid::new("sci-layout").num_columns(2).show(ui, |ui| {
            ui.label("列数");
            ui.add(egui::DragValue::new(&mut s.columns).range(1..=100));
            ui.end_row();
            ui.label("列间距");
            ui.add(egui::DragValue::new(&mut s.gap_x).range(0.0..=100_000.0));
            ui.end_row();
            ui.label("行间距");
            ui.add(egui::DragValue::new(&mut s.gap_y).range(0.0..=100_000.0));
            ui.end_row();
            ui.label("宽度");
            ui.add(egui::TextEdit::singleline(&mut s.width).desired_width(90.0).hint_text("留空不变"));
            ui.end_row();
            ui.label("高度");
            ui.add(egui::TextEdit::singleline(&mut s.height).desired_width(90.0).hint_text("留空不变"));
            ui.end_row();
        });
        ui.checkbox(&mut s.keep_ratio, "保持比例（只填宽或高）");
        ui.checkbox(&mut s.scale_strokes, "缩放描边与效果");
        ui.horizontal(|ui| {
            if ui.add_enabled(count > 0, egui::Button::new("网格排图")).clicked() {
                action(app, &s, "grid", "");
            }
            if ui.add_enabled(count > 0, egui::Button::new("统一尺寸")).clicked() {
                action(app, &s, "resize", "");
            }
        });
    });
    egui::CollapsingHeader::new("对齐与等间距").show(ui, |ui| {
        ui.checkbox(&mut s.align_first, "以第一个对象为参考");
        ui.horizontal(|ui| {
            for (axis, label) in [("left", "左"), ("center", "水平居中"), ("right", "右")] {
                if ui.button(label).clicked() {
                    action(app, &s, "align", axis);
                }
            }
        });
        ui.horizontal(|ui| {
            for (axis, label) in [("top", "顶"), ("middle", "垂直居中"), ("bottom", "底")] {
                if ui.button(label).clicked() {
                    action(app, &s, "align", axis);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("净间距");
            ui.add(egui::DragValue::new(&mut s.spacing).range(0.0..=100_000.0));
        });
        ui.horizontal(|ui| {
            if ui.button("水平等间距").clicked() {
                action(app, &s, "spacing", "horizontal");
            }
            if ui.button("垂直等间距").clicked() {
                action(app, &s, "spacing", "vertical");
            }
        });
    });
    super::advanced::show(app, ui, &mut s);
    egui::CollapsingHeader::new("子图标签").default_open(true).show(ui, |ui| {
        egui::Grid::new("sci-labels").num_columns(2).show(ui, |ui| {
            ui.label("格式");
            choice(
                ui,
                "sci-style",
                &mut s.label_style,
                &[
                    ("(A)", "(A) (B)"),
                    ("(a)", "(a) (b)"),
                    ("A", "A B C"),
                    ("a", "a b c"),
                    ("A)", "A) B) C)"),
                    ("a)", "a) b) c)"),
                    ("1", "1 2 3"),
                    ("1)", "1) 2) 3)"),
                ],
            );
            ui.end_row();
            ui.label("起始编号");
            ui.add(egui::DragValue::new(&mut s.label_start).range(1..=9999));
            ui.end_row();
            ui.label("粗体");
            ui.checkbox(&mut s.label_bold, "启用");
            ui.end_row();
            ui.label("字号");
            ui.add(egui::DragValue::new(&mut s.label_size).range(1.0..=300.0));
            ui.end_row();
            ui.label("字体");
            ui.add(egui::TextEdit::singleline(&mut s.font).desired_width(130.0));
            ui.end_row();
            ui.label("颜色");
            super::colors::field(app, ui, &mut s.label_color, "labels");
            ui.end_row();
            ui.label("锚点");
            choice(
                ui,
                "sci-anchor",
                &mut s.label_anchor,
                &[("top-left", "左上"), ("top-right", "右上"), ("bottom-left", "左下"), ("bottom-right", "右下")],
            );
            ui.end_row();
            let before = [s.dx, s.dy];
            ui.label("横向偏移");
            let dx = ui.add(egui::DragValue::new(&mut s.dx).range(-100_000.0..=100_000.0));
            ui.end_row();
            ui.label("纵向偏移");
            let dy = ui.add(egui::DragValue::new(&mut s.dy).range(-100_000.0..=100_000.0));
            super::advanced::offset_preview(app, ui, &mut s, before, &dx, &dy);
            ui.end_row();
        });
        if ui.add_enabled(count > 0, egui::Button::new("添加 / 更新标签")).clicked() {
            action(app, &s, "labels", "");
        }
        ui.checkbox(&mut s.live_labels, "实时调整已有标签偏移");
        ui.small("选择原图；实时模式只移动已有标签。Esc取消本次偏移。");
    });
    egui::CollapsingHeader::new("位置管理").show(ui, |ui| {
        ui.checkbox(&mut s.relative_position, "复制相对位置（首对象作参考）");
        ui.horizontal(|ui| {
            if ui.button("复制位置").clicked() {
                action(app, &s, "copy_position", "");
            }
            if ui.add_enabled(app.ui.scientific_clipboard.is_some(), egui::Button::new("粘贴位置")).clicked() {
                action(app, &s, "paste_position", "");
            }
            if ui.button("交换位置").clicked() {
                action(app, &s, "swap", "");
            }
        });
    });
    egui::CollapsingHeader::new("图片边框").show(ui, |ui| {
        ui.checkbox(&mut s.border_group, "与原图编组（一起移动缩放）");
        ui.horizontal(|ui| {
            ui.label("虚线间隔 pt（0为实线）");
            ui.add(egui::DragValue::new(&mut s.border_dash).range(0.0..=10000.0));
        });
        ui.horizontal(|ui| {
            ui.label("线宽");
            ui.add(egui::DragValue::new(&mut s.border_weight).range(0.01..=100.0));
        });
        ui.horizontal(|ui| {
            ui.label("颜色");
            super::colors::field(app, ui, &mut s.border_color, "borders");
        });
        if ui.button("添加 / 更新边框").clicked() {
            action(app, &s, "borders", "");
        }
    });
    super::scale::show(app, ui, &mut s);
    super::zoom::show(app, ui, &mut s);
    super::layers::show(app, ui, &mut s);
    app.ui.scientific = s.clone();
    super::palette::show(app, ui);
    s.label_color = app.ui.scientific.label_color.clone();
    s.border_color = app.ui.scientific.border_color.clone();
    egui::CollapsingHeader::new("命名预设").show(ui, |ui| {
        ui.add(egui::TextEdit::singleline(&mut s.preset_name).desired_width(170.0).hint_text("预设名称"));
        egui::ComboBox::from_id_salt("sci-presets").selected_text("已保存的预设").show_ui(ui, |ui| {
            for name in app.ui.scientific_presets.keys() {
                ui.selectable_value(&mut s.preset_name, name.clone(), name);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("保存").clicked() {
                action(app, &s, "preset_save", "");
            }
            if ui.button("载入").clicked() {
                action(app, &s, "preset_load", "");
                s = app.ui.scientific.clone();
            }
            if ui.button("删除").clicked() {
                action(app, &s, "preset_delete", "");
            }
        });
    });
    ui.separator();
    ui.label(&app.ui.status);
    super::advanced::finish_offsets(app, ui.ctx(), &mut s);
    app.ui.scientific = s;
}
