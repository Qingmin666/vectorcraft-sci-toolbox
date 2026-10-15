//! Scientific arrangement geometry, independent of a particular host application's scripts.
use super::*;

pub(super) fn fractions(anchor: &str) -> Result<(f64, f64), String> {
    Ok(match anchor {
        "top-left" => (0., 0.),
        "top-center" => (0.5, 0.),
        "top-right" => (1., 0.),
        "middle-left" => (0., 0.5),
        "center" => (0.5, 0.5),
        "middle-right" => (1., 0.5),
        "bottom-left" => (0., 1.),
        "bottom-center" => (0.5, 1.),
        "bottom-right" => (1., 1.),
        _ => return Err("未知的参考锚点".into()),
    })
}

pub(super) fn order(app: &mut VectorcraftApp, boxes: &mut [BoxInfo], settings: &Settings) -> Result<(), String> {
    if settings.order == "stacking" {
        let doc = app.run("document.inspect", json!({}))?;
        fn visit(n: &Value, ranks: &mut BTreeMap<u64, usize>) {
            if let Some(id) = n.get("id").and_then(Value::as_u64) {
                ranks.insert(id, ranks.len());
            }
            for child in n.get("children").and_then(Value::as_array).into_iter().flatten() {
                visit(child, ranks);
            }
        }
        let mut ranks = BTreeMap::new();
        for layer in doc.get("layers").and_then(Value::as_array).into_iter().flatten() {
            visit(layer, &mut ranks);
        }
        boxes.sort_by_key(|b| ranks.get(&b.id).copied().unwrap_or(usize::MAX));
    } else {
        extended::ordered(boxes, &settings.order)?;
    }
    if settings.reverse_order {
        boxes.reverse();
    }
    Ok(())
}

pub(super) fn origins(app: &mut VectorcraftApp, boxes: &[BoxInfo], anchor: &str) -> Result<Vec<[f64; 2]>, String> {
    let (fx, fy) = fractions(anchor)?;
    let doc = app.run("document.inspect", json!({}))?;
    let boards = doc.get("artboards").and_then(Value::as_array).ok_or("没有画板")?;
    boxes
        .iter()
        .map(|b| {
            let center = (b.x + b.w / 2., b.y + b.h / 2.);
            let board = boards
                .iter()
                .find(|a| {
                    let x = a.get("x").and_then(Value::as_f64).unwrap_or(0.);
                    let y = a.get("y").and_then(Value::as_f64).unwrap_or(0.);
                    let w = a.get("width").and_then(Value::as_f64).unwrap_or(0.);
                    let h = a.get("height").and_then(Value::as_f64).unwrap_or(0.);
                    center.0 >= x && center.0 <= x + w && center.1 >= y && center.1 <= y + h
                })
                .or_else(|| boards.first())
                .ok_or("没有参考画板")?;
            let f = |key| board.get(key).and_then(Value::as_f64).ok_or("画板边界无效");
            Ok([f("x")? + fx * f("width")?, f("y")? + fy * f("height")?])
        })
        .collect()
}

pub(super) fn measure(app: &mut VectorcraftApp, s: &mut Settings, op: &str, axis: &str) -> Result<(), String> {
    let (mut boxes, _) = selection(app)?;
    if op == "copy_spacing" {
        extended::ordered(&mut boxes, axis)?;
    } else {
        order(app, &mut boxes, s)?;
    }
    let first = boxes.first().ok_or("请选择参考对象")?;
    if op == "copy_size" {
        s.width = format!("{:.6}", first.w);
        s.height = format!("{:.6}", first.h);
        s.keep_ratio = false;
    } else {
        let second = boxes.get(1).ok_or("请选择两个参考对象")?;
        s.spacing = match axis {
            "horizontal" => second.x - first.x - first.w,
            "vertical" => second.y - first.y - first.h,
            _ => return Err("未知的间距方向".into()),
        };
        if !s.spacing.is_finite() || s.spacing < 0. {
            return Err("参考对象重叠或顺序相反".into());
        }
    }
    Ok(())
}

pub(super) fn filter(app: &mut VectorcraftApp, op: &str) -> Result<(), String> {
    let doc = app.run("document.inspect", json!({}))?;
    let selected = doc.get("selection").and_then(Value::as_array).ok_or("没有选择对象")?;
    fn texts(n: &Value, out: &mut BTreeMap<u64, bool>) {
        if let Some(id) = n.get("id").and_then(Value::as_u64) {
            out.insert(id, n.get("kind").and_then(Value::as_str) == Some("Type"));
        }
        for child in n.get("children").and_then(Value::as_array).into_iter().flatten() {
            texts(child, out);
        }
    }
    let mut kinds = BTreeMap::new();
    for n in doc.get("layers").and_then(Value::as_array).into_iter().flatten() {
        texts(n, &mut kinds);
    }
    let ids: Vec<_> =
        selected.iter().filter_map(Value::as_u64).filter(|id| kinds.get(id).copied().unwrap_or(false) == (op == "filter_text")).collect();
    app.run("select.set", json!({"ids":ids}))?;
    Ok(())
}

pub(super) fn extra_plan(boxes: &[BoxInfo], s: &Settings, op: &str, axis: &str) -> Result<Vec<BoxInfo>, String> {
    let mut result = boxes.to_vec();
    match op {
        "swap" => {
            if boxes.len() != 2 {
                return Err("请恰好选择两个对象".into());
            }
            let a = boxes.first().ok_or("缺少对象")?;
            let b = boxes.get(1).ok_or("缺少对象")?;
            let (fx, fy) = fractions(&s.swap_anchor)?;
            if let Some(p) = result.first_mut() {
                p.x = b.x + fx * (b.w - a.w);
                p.y = b.y + fy * (b.h - a.h);
            }
            if let Some(p) = result.get_mut(1) {
                p.x = a.x + fx * (a.w - b.w);
                p.y = a.y + fy * (a.h - b.h);
            }
        }
        "distribute" => {
            if boxes.len() < 3 {
                return Err("分布至少需要三个对象".into());
            }
            let horizontal = axis.starts_with("h-");
            let f = match axis {
                "h-left" | "v-top" => 0.,
                "h-center" | "v-center" => 0.5,
                "h-right" | "v-bottom" => 1.,
                _ => return Err("未知的分布方向".into()),
            };
            let position = |b: &BoxInfo| if horizontal { b.x + f * b.w } else { b.y + f * b.h };
            result.sort_by(|a, b| position(a).total_cmp(&position(b)));
            let start = result.first().map(position).ok_or("没有对象")?;
            let end = result.last().map(position).ok_or("没有对象")?;
            let step = (end - start) / (result.len() - 1) as f64;
            for (i, b) in result.iter_mut().enumerate() {
                if horizontal {
                    b.x = start + i as f64 * step - f * b.w;
                } else {
                    b.y = start + i as f64 * step - f * b.h;
                }
            }
        }
        "spacing_auto" => {
            if boxes.len() < 2 {
                return Err("设置间距至少需要两个对象".into());
            }
            let horizontal = match axis {
                "horizontal" => true,
                "vertical" => false,
                _ => return Err("未知的间距方向".into()),
            };
            result.sort_by(|a, b| if horizontal { a.x.total_cmp(&b.x) } else { a.y.total_cmp(&b.y) });
            let first = result.first().ok_or("没有对象")?;
            let last = result.last().ok_or("没有对象")?;
            let start = if horizontal { first.x } else { first.y };
            let end = if horizontal { last.x + last.w } else { last.y + last.h };
            let total: f64 = result.iter().map(|b| if horizontal { b.w } else { b.h }).sum();
            let gap = (end - start - total) / (result.len() - 1) as f64;
            if gap < 0. {
                return Err("当前边界内无法分布不重叠的对象".into());
            }
            let mut cursor = start;
            for b in &mut result {
                if horizontal {
                    b.x = cursor;
                    cursor += b.w + gap;
                } else {
                    b.y = cursor;
                    cursor += b.h + gap;
                }
            }
        }
        _ => return Err("未知的布局操作".into()),
    }
    Ok(result)
}

pub(super) fn grid(boxes: &[BoxInfo], s: &Settings, original: &[BoxInfo]) -> Result<Vec<BoxInfo>, String> {
    let old: BTreeMap<_, _> = original.iter().map(|b| (b.id, b)).collect();
    let mut source = boxes.to_vec();
    let mut rows: Vec<Vec<BoxInfo>> = Vec::new();
    if s.auto_layout {
        source.sort_by(|a, b| {
            let a = old.get(&a.id).copied().unwrap_or(a);
            let b = old.get(&b.id).copied().unwrap_or(b);
            a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x))
        });
        for b in source {
            if let Some(row) = rows.last_mut() {
                let first = row.first().ok_or("排布中存在空行")?;
                let first = old.get(&first.id).copied().unwrap_or(first);
                let reference = old.get(&b.id).copied().unwrap_or(&b);
                if (reference.y + reference.h).min(first.y + first.h) - reference.y.max(first.y) >= reference.h.min(first.h) * 0.5 {
                    row.push(b);
                    continue;
                }
            }
            rows.push(vec![b]);
        }
    } else {
        for row in source.chunks(s.columns.max(1)) {
            rows.push(row.to_vec());
        }
    }
    let left = boxes.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
    let top = boxes.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
    let right = boxes.iter().map(|b| b.x + b.w).fold(f64::NEG_INFINITY, f64::max);
    let width = if s.layout_width.trim().is_empty() || s.layout_width.trim() == "0" {
        right - left
    } else {
        s.layout_width.parse::<f64>().ok().filter(|n| n.is_finite() && (0.01..=100000.).contains(n)).ok_or("布局总宽度无效")?
    };
    let _single_column = rows.iter().all(|r| r.len() == 1);
    let _reference_width = boxes.first().map(|b| b.w).unwrap_or(1.);
    let mut out = Vec::new();
    let mut y = top;
    for mut row in rows {
        if s.auto_layout {
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
            if s.reverse_order {
                row.reverse();
            }
        }
        let n = row.len();
        let mut gap = s.gap_x;
        if s.size_mode == "auto" {
            let base = row.first().ok_or("排布中存在空行")?;
            let h = if s.align_edges { (width - gap * (n - 1) as f64) / row.iter().map(|b| b.w / b.h).sum::<f64>() } else { base.h };
            if h <= 0. || !h.is_finite() {
                return Err("布局总宽度不足以容纳列间距".into());
            }
            for b in &mut row {
                b.w *= h / b.h;
                b.h = h;
            }
        } else if s.align_edges && n > 1 {
            gap = (width - row.iter().map(|b| b.w).sum::<f64>()) / (n - 1) as f64;
            if gap < 0. {
                return Err("布局总宽度不足，请使用自动尺寸适配".into());
            }
        }
        let height = row.iter().map(|b| b.h).fold(0., f64::max);
        let mut x = left;
        for mut b in row {
            b.x = x;
            b.y = y;
            x += b.w + gap;
            out.push(b);
        }
        y += height + s.gap_y;
    }
    Ok(out)
}

pub(super) fn renumber(app: &mut VectorcraftApp, s: &Settings) -> Result<(), String> {
    let (mut boxes, nodes) = selection(app)?;
    order(app, &mut boxes, s)?;
    if boxes.iter().any(|b| nodes.get(&b.id).and_then(|n| n.get("kind")).and_then(Value::as_str) != Some("Type")) {
        return Err("请仅选择文字标签".into());
    }
    extended::valid_color(&s.label_color)?;
    if s.label_start == 0 || s.label_start > 9999 || !s.label_size.is_finite() || !(1.0..=300.).contains(&s.label_size) {
        return Err("标签参数无效".into());
    }
    label_text(s.label_start, &s.label_style)?;
    transaction(app, "更新所选标签编号", boxes.iter().map(|b| b.id).collect(), |app| {
        for (i, b) in boxes.iter().enumerate() {
            app.run("text.setText", json!({"ids":[b.id],"text":label_text(s.label_start+i,&s.label_style)?}))?;
            app.run(
                "text.setStyle",
                json!({"ids":[b.id],"font":s.font,"style":if s.label_bold{"Bold"}else{"Regular"},"size":s.label_size,"fill":s.label_color}),
            )?;
        }
        Ok(())
    })
}

fn act(app: &mut VectorcraftApp, s: &mut Settings, op: &str, axis: &str) {
    match app.run("ui.scientific", json!({"operation":op,"axis":axis,"settings":s})) {
        Ok(_) => {
            *s = app.ui.scientific.clone();
            app.status("科研操作完成");
        }
        Err(e) => app.status(e),
    }
}

pub(super) fn show(app: &mut VectorcraftApp, ui: &mut Ui, s: &mut Settings) {
    egui::CollapsingHeader::new("高级排版与操作").show(ui, |ui| {
        ui.checkbox(&mut s.reverse_order, "反向排序");
        ui.checkbox(&mut s.auto_layout, "识别当前行列布局");
        egui::ComboBox::from_id_salt("sci-size-mode")
            .selected_text(match s.size_mode.as_str() {
                "auto" => "同行等高自动缩放",
                "original" => "保持原大小",
                _ => "使用上方宽高参数",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut s.size_mode, "custom".into(), "使用上方宽高参数");
                ui.selectable_value(&mut s.size_mode, "original".into(), "保持原大小");
                ui.selectable_value(&mut s.size_mode, "auto".into(), "同行等高自动缩放");
            });
        ui.checkbox(&mut s.align_edges, "各行左右边缘对齐");
        ui.horizontal(|ui| {
            ui.label("总宽度 pt");
            ui.add(egui::TextEdit::singleline(&mut s.layout_width).desired_width(90.).hint_text("空或0保持"));
        });
        if ui.button("执行高级排图").clicked() {
            act(app, s, "grid", "");
        }
        ui.horizontal(|ui| {
            if ui.button("读取所选宽高").clicked() {
                act(app, s, "copy_size", "");
            }
            if ui.button("粘贴尺寸").clicked() {
                act(app, s, "resize", "");
            }
        });
        ui.horizontal(|ui| {
            if ui.button("读取水平间距").clicked() {
                act(app, s, "copy_spacing", "horizontal");
            }
            if ui.button("读取垂直间距").clicked() {
                act(app, s, "copy_spacing", "vertical");
            }
        });
        ui.horizontal(|ui| {
            if ui.button("自动均匀水平净间距").clicked() {
                act(app, s, "spacing_auto", "horizontal");
            }
            if ui.button("自动均匀垂直净间距").clicked() {
                act(app, s, "spacing_auto", "vertical");
            }
        });
        ui.label("固定两端，均分边缘或中心");
        ui.horizontal_wrapped(|ui| {
            for (axis, title) in [
                ("h-left", "左边缘"),
                ("h-center", "水平中心"),
                ("h-right", "右边缘"),
                ("v-top", "顶边缘"),
                ("v-center", "垂直中心"),
                ("v-bottom", "底边缘"),
            ] {
                if ui.button(title).clicked() {
                    act(app, s, "distribute", axis);
                }
            }
        });
        if ui.button("水平与垂直同时居中").clicked() {
            act(app, s, "align", "both");
        }
        for (id, title, value) in [("swap", "交换参考点", &mut s.swap_anchor), ("position", "位置复制参考点", &mut s.position_anchor)] {
            egui::ComboBox::from_id_salt(("sci-anchor9", id)).selected_text(format!("{title}: {value}")).show_ui(ui, |ui| {
                for (v, n) in [
                    ("top-left", "左上"),
                    ("top-center", "上中"),
                    ("top-right", "右上"),
                    ("middle-left", "左中"),
                    ("center", "中心"),
                    ("middle-right", "右中"),
                    ("bottom-left", "左下"),
                    ("bottom-center", "下中"),
                    ("bottom-right", "右下"),
                ] {
                    ui.selectable_value(value, v.into(), n);
                }
            });
        }
        ui.checkbox(&mut s.artboard_reference, "位置复制以各对象所在画板为参考");
        ui.checkbox(&mut s.allow_position_mismatch, "数量不同时循环应用位置模式");
        ui.checkbox(&mut s.position_reverse, "相对偏移反向");
        ui.checkbox(&mut s.position_override, "手动覆盖坐标 / 相对偏移");
        if s.position_override {
            ui.horizontal(|ui| {
                ui.label("X pt");
                ui.add(egui::DragValue::new(&mut s.position_dx));
                ui.label("Y pt");
                ui.add(egui::DragValue::new(&mut s.position_dy));
            });
        }
        ui.checkbox(&mut s.spacing_from_end, "粘贴间距时固定最右 / 最下对象");

        ui.small("画板按对象中心判断；画板外对象使用第一画板。");
        ui.horizontal(|ui| {
            if ui.button("只保留文字选择").clicked() {
                act(app, s, "filter_text", "");
            }
            if ui.button("排除文字选择").clicked() {
                act(app, s, "exclude_text", "");
            }
        });
        if ui.button("更新所选文字的编号与样式").clicked() {
            act(app, s, "renumber_labels", "");
        }
    });
}

#[derive(Clone)]
struct OffsetPreview {
    start: [f64; 2],
    ids: Vec<u64>,
    doc: Option<usize>,
    fields: [egui::Id; 2],
    title: String,
}

pub(super) fn offset_preview(app: &mut VectorcraftApp, ui: &Ui, s: &mut Settings, before: [f64; 2], dx: &egui::Response, dy: &egui::Response) {
    if !s.live_labels || !(dx.changed() || dy.changed()) {
        return;
    }
    let mut state: Option<OffsetPreview> = crate::panels::pstate(ui.ctx(), "sci-offset-preview");
    if state.is_none() {
        if app.session.in_interaction() {
            app.status("请先结束当前编辑");
            return;
        }
        let result = (|| -> Result<Vec<u64>, String> {
            let (boxes, nodes) = selection(app)?;
            let mut ids = Vec::new();
            for (id, n) in &nodes {
                if !n.get("name").and_then(Value::as_str).unwrap_or("").starts_with("[SCI label]") {
                    continue;
                }
                let detail = app.run("document.node", json!({"id":id}))?;
                if attribute(&detail, "sci-target").and_then(|v| v.parse::<u64>().ok()).is_some_and(|target| boxes.iter().any(|b| b.id == target)) {
                    if n.get("sciBlocked").and_then(Value::as_bool).unwrap_or(false) {
                        return Err("请先解锁关联标签".into());
                    }
                    ids.push(*id);
                }
            }
            if ids.is_empty() {
                return Err("请先为所选原图添加标签".into());
            }
            Ok(ids)
        })();
        match result {
            Ok(ids) => {
                state = Some(OffsetPreview {
                    start: before,
                    ids,
                    doc: app.session.active_index(),
                    fields: [dx.id, dy.id],
                    title: app.session.active().map(|st| st.title().to_string()).unwrap_or_default(),
                })
            }
            Err(e) => {
                app.status(e);
                return;
            }
        }
    }
    if let Some(state) = state.as_ref()
        && state.doc == app.session.active_index()
    {
        crate::panels::live_run(
            app,
            "实时调整标签偏移",
            "object.transform",
            json!({"ids":state.ids,"matrix":[1,0,0,1,s.dx-state.start[0],s.dy-state.start[1]],"patterns":true}),
            crate::widgets::Live::Dragging,
        );
    }
    crate::panels::set_pstate(ui.ctx(), "sci-offset-preview", state);
}

pub(super) fn finish_offsets(app: &mut VectorcraftApp, ctx: &egui::Context, s: &mut Settings) {
    let state: Option<OffsetPreview> = crate::panels::pstate(ctx, "sci-offset-preview");
    let Some(state) = state else { return };
    let cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let focused = ctx.memory(|m| m.focused());
    let idle = !ctx.input(|i| i.pointer.any_down()) && !state.fields.iter().any(|id| Some(*id) == focused);
    let changed_doc = state.doc != app.session.active_index() || app.session.active().is_none_or(|st| st.title() != state.title);
    if cancel || idle || changed_doc {
        if changed_doc {
            let current = app.session.active_index();
            if let Some(old) = state.doc
                && app.session.set_active(old)
                && app.session.active().is_some_and(|st| st.title() == state.title)
                && app.session.in_interaction()
                && let Err(e) = app.session.cancel_interaction()
            {
                app.status(e.to_string());
            }
            if let Some(current) = current {
                app.session.set_active(current);
            }
        }
        if cancel || changed_doc {
            s.dx = state.start[0];
            s.dy = state.start[1];
        }

        if state.doc == app.session.active_index() && app.session.in_interaction() {
            let result = if cancel { app.session.cancel_interaction() } else { app.session.commit_interaction() };
            if let Err(e) = result {
                app.status(e.to_string());
            }
            if cancel {
                s.dx = state.start[0];
                s.dy = state.start[1];
            }
            app.sync_views();
        }
        crate::panels::set_pstate(ctx, "sci-offset-preview", Option::<OffsetPreview>::None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn boxes() -> Vec<BoxInfo> {
        vec![
            BoxInfo { id: 1, x: 0., y: 0., w: 20., h: 10. },
            BoxInfo { id: 2, x: 80., y: 1., w: 40., h: 20. },
            BoxInfo { id: 3, x: 4., y: 90., w: 30., h: 10. },
        ]
    }
    #[test]
    fn centers_swap_and_endpoints_stay_fixed() {
        let b = boxes();
        let s = Settings { swap_anchor: "center".into(), ..Default::default() };
        let p = extra_plan(&b[..2], &s, "swap", "").unwrap();
        assert_eq!(p[0].x + p[0].w / 2., b[1].x + b[1].w / 2.);
        let p = extra_plan(&b, &s, "distribute", "h-center").unwrap();
        assert_eq!(p.first().unwrap().x, 0.);
        assert_eq!(p.last().unwrap().x, 80.);
    }
    #[test]
    fn inferred_rows_fill_a_fixed_width_without_distortion() {
        let s = Settings {
            auto_layout: true,
            size_mode: "auto".into(),
            align_edges: true,
            layout_width: "200".into(),
            gap_x: 10.,
            gap_y: 20.,
            ..Default::default()
        };
        let p = grid(&boxes(), &s, &boxes()).unwrap();
        assert_eq!(p.len(), 3);
        assert!((p[1].x + p[1].w - 200.).abs() < 1e-8);
        assert!((p[0].w / p[0].h - 2.).abs() < 1e-8);
        assert!(p[2].y > p[0].y + p[0].h);
    }
}
