//! Real-world calibration and editable scale bars. Printing DPI is never calibration.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScaleSettings {
    pub fov_width: String,
    pub fov_height: String,
    pub fov_unit: String,
    pub length: f64,
    pub unit: String,
    pub vertical: bool,
    pub thickness: f64,
    pub color: String,
    pub font_color: String,
    pub font_size: f64,
    pub bold: bool,
    pub show_text: bool,
    pub corner: String,
    pub offset: f64,
    pub auto_group: bool,
    pub override_batch: bool,
    pub auto_save: bool,
}
impl Default for ScaleSettings {
    fn default() -> Self {
        Self {
            fov_width: String::new(),
            fov_height: String::new(),
            fov_unit: "um".into(),
            length: 10.,
            unit: "um".into(),
            vertical: false,
            thickness: 2.,
            color: "#FFFFFF".into(),
            font_color: "#FFFFFF".into(),
            font_size: 10.,
            bold: false,
            show_text: true,
            corner: "bottom-right".into(),
            offset: 6.,
            auto_group: true,
            override_batch: false,
            auto_save: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Fov {
    pub width: Option<f64>,
    pub height: Option<f64>,
}

pub(super) fn factor(unit: &str) -> Result<f64, String> {
    match unit {
        "nm" => Ok(0.001),
        "um" | "μm" | "µm" => Ok(1.),
        "mm" => Ok(1000.),
        "cm" => Ok(10000.),
        "m" => Ok(1_000_000.),
        "inch" => Ok(25400.),
        _ => Err("未知的物理单位".into()),
    }
}
fn dimension(value: &str, factor: f64) -> Result<Option<f64>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0. && *n <= 1e12)
        .map(|n| Some(n * factor))
        .ok_or("真实视野必须为有限的正数".into())
}
fn input_fov(s: &ScaleSettings) -> Result<Fov, String> {
    let f = factor(&s.fov_unit)?;
    Ok(Fov { width: dimension(&s.fov_width, f)?, height: dimension(&s.fov_height, f)? })
}

pub(super) fn stored(app: &mut VectorcraftApp, id: u64) -> Result<Option<Fov>, String> {
    let detail = app.run("document.node", json!({"id":id}))?;
    attribute(&detail, "sci-fov").map(|raw| serde_json::from_str(raw).map_err(|_| "Stored FOV is invalid".into())).transpose()
}

pub(super) fn targets(app: &mut VectorcraftApp) -> Result<(Vec<BoxInfo>, BTreeMap<u64, Value>), String> {
    let (mut boxes, nodes) = selection(app)?;
    for b in &mut boxes {
        let detail = app.run("document.node", json!({"id":b.id}))?;
        if matches!(attribute(&detail, "sci-role"), Some("scale-wrapper" | "border-wrapper")) {
            let id = attribute(&detail, "sci-target").and_then(|s| s.parse::<u64>().ok()).ok_or("标注组合的关联已损坏")?;
            *b = bounds(nodes.get(&id).ok_or("原始源对象已被移除")?)?;
        }
        if nodes.get(&b.id).and_then(|n| n.get("sciBlocked")).and_then(Value::as_bool).unwrap_or(false) {
            return Err("请先解锁已标定源对象".into());
        }
    }
    Ok((boxes, nodes))
}

fn oriented(app: &VectorcraftApp, b: &BoxInfo) -> Result<(BoxInfo, f64), String> {
    let frame = app.session.active().and_then(|st| st.doc.oriented_bounds(&[vectorcraft_doc::NodeId(b.id)], false)).ok_or("找不到源对象边界")?;
    let center = frame.center();
    if frame.rect.width() <= 1e-7 || frame.rect.height() <= 1e-7 {
        return Err("源对象变换不可逆".into());
    }
    Ok((
        BoxInfo {
            id: b.id,
            x: center.x - frame.rect.width() / 2.,
            y: center.y - frame.rect.height() / 2.,
            w: frame.rect.width(),
            h: frame.rect.height(),
        },
        frame.angle,
    ))
}

fn physical_length(s: &ScaleSettings, f: &Fov, display: &BoxInfo) -> Result<f64, String> {
    if !s.length.is_finite()
        || s.length < 0.
        || !s.thickness.is_finite()
        || !(0.1..=100.).contains(&s.thickness)
        || !s.font_size.is_finite()
        || !(1.0..=300.).contains(&s.font_size)
        || !s.offset.is_finite()
        || !(0.0..=10000.).contains(&s.offset)
    {
        return Err("比例尺样式或长度无效".into());
    }
    let fov = if s.vertical { f.height } else { f.width }.ok_or("请输入比例尺方向对应的真实视野")?;
    let length = s.length * factor(&s.unit)?;
    if !fov.is_finite() || fov <= 0. || length > fov {
        return Err("比例尺长度不能超过标定视野".into());
    }
    Ok(length / fov * if s.vertical { display.h } else { display.w })
}

pub(super) fn fov_for(app: &mut VectorcraftApp, b: &BoxInfo, s: &ScaleSettings, multiple: bool, nodes: &BTreeMap<u64, Value>) -> Result<Fov, String> {
    let existing = stored(app, b.id)?;
    let input = input_fov(s)?;
    if multiple && !s.override_batch {
        if let Some(fov) = existing {
            return Ok(fov);
        }
        return super::tiff::image_fov(app, b, nodes)
            .map_err(|_| "A batch image has no calibration; calibrate each image or explicitly enable batch override".into());
    }
    if input.width.is_some() || input.height.is_some() { Ok(input) } else { existing.ok_or("请输入真实视野或读取图像标定".into()) }
}

pub(super) fn apply(app: &mut VectorcraftApp, s: &ScaleSettings, calibration_only: bool) -> Result<(), String> {
    let (boxes, nodes) = targets(app)?;
    extended::valid_color(&s.color)?;
    extended::valid_color(&s.font_color)?;
    advanced::fractions(&s.corner)?;
    let mut plans = Vec::new();
    for b in &boxes {
        let fov = fov_for(app, b, s, boxes.len() > 1, &nodes)?;
        if !calibration_only {
            physical_length(s, &fov, &oriented(app, b)?.0)?;
        }
        plans.push((b.clone(), fov));
    }
    let mut previous = BTreeMap::new();
    for (id, n) in &nodes {
        if !n.get("name").and_then(Value::as_str).unwrap_or("").starts_with("[SCI scale]") {
            continue;
        }
        let d = app.run("document.node", json!({"id":id}))?;
        if let Some(target) = attribute(&d, "sci-target").and_then(|s| s.parse::<u64>().ok())
            && boxes.iter().any(|b| b.id == target)
        {
            if n.get("sciBlocked").and_then(Value::as_bool).unwrap_or(false) {
                return Err("请先解锁已有比例尺".into());
            }
            previous.entry(target).or_insert_with(Vec::new).push(*id);
        }
    }
    transaction(app, if calibration_only { "保存真实标定" } else { "添加或更新比例尺" }, boxes.iter().map(|b| b.id).collect(), |app| {
        for (b, fov) in &plans {
            app.run("object.setProps",json!({"ids":[b.id],"data":{"sci-fov":serde_json::to_string(fov).map_err(|e|e.to_string())?,"sci-scale-options":serde_json::to_string(s).map_err(|e|e.to_string())?}}))?;
            if calibration_only {
                continue;
            }
            if let Some(old) = previous.get(&b.id) {
                app.run("edit.clear", json!({"ids":old}))?;
            }
            if s.length == 0. {
                continue;
            }
            draw(app, b, fov, s)?;
        }
        Ok(())
    })
}

pub(super) fn draw(app: &mut VectorcraftApp, b: &BoxInfo, fov: &Fov, s: &ScaleSettings) -> Result<u64, String> {
    let (plane, angle) = oriented(app, b)?;
    let b = &plane;
    let length = physical_length(s, fov, b)?;
    let (fx, fy) = advanced::fractions(&s.corner)?;
    let w = if s.vertical { s.thickness } else { length };
    let h = if s.vertical { length } else { s.thickness };
    let x = b.x + fx * b.w - fx * w + (1. - 2. * fx) * s.offset;
    let y = b.y + fy * b.h - fy * h + (1. - 2. * fy) * s.offset;
    let bar = app.run("shape.rectangle", json!({"x":x,"y":y,"width":w,"height":h}))?.get("id").and_then(Value::as_u64).ok_or("缺少比例尺")?;
    app.run("paint.setFill", json!({"ids":[bar],"color":s.color}))?;
    app.run("paint.setStroke", json!({"ids":[bar],"none":true}))?;
    let mut ids = vec![bar];
    if s.show_text {
        let text = format!("{} {}", s.length, if s.unit == "um" { "μm" } else { &s.unit });
        let label = app
            .run("text.create", json!({"x":x,"y":y,"text":text,"size":s.font_size,"font":"Arial","color":s.font_color}))?
            .get("id")
            .and_then(Value::as_u64)
            .ok_or("缺少比例尺文字")?;
        app.run("text.setStyle", json!({"ids":[label],"style":if s.bold{"Bold"}else{"Regular"}}))?;
        let actual = bounds(&app.run("document.node", json!({"id":label,"summary":true}))?)?;
        let gap = s.thickness.max(2.);
        let tx = if s.vertical { if fx >= 0.5 { x - gap - actual.w } else { x + w + gap } } else { if fx >= 0.5 { x + w - actual.w } else { x } };
        let ty = if s.vertical { if fy >= 0.5 { y + h - actual.h } else { y } } else { if fy >= 0.5 { y - gap - actual.h } else { y + h + gap } };
        move_object(app, &actual, tx, ty)?;
        ids.push(label);
    }
    app.run("select.set", json!({"ids":ids}))?;
    let group = app.run("object.group", json!({}))?.get("id").and_then(Value::as_u64).ok_or("缺少比例尺组合")?;
    app.run(
        "object.setProps",
        json!({"ids":[group],"name":format!("[SCI scale] {}",b.id),"data":{"sci-role":"scale","sci-target":b.id.to_string()}}),
    )?;
    if angle.abs() > 1e-8 {
        app.run("select.set", json!({"ids":[group]}))?;
        app.run("object.rotate", json!({"angle":angle,"origin":[b.x+b.w/2.,b.y+b.h/2.]}))?;
    }
    if s.auto_group {
        let parent = app.session.active().and_then(|st| st.doc.parent_of(vectorcraft_doc::NodeId(b.id)));
        let existing = if let Some(parent) = parent {
            let d = app.run("document.node", json!({"id":parent.0}))?;
            matches!(attribute(&d, "sci-role"), Some("scale-wrapper" | "border-wrapper"))
        } else {
            false
        };
        // Existing wrappers are reused by grouping the source and new bar; outer wrappers remain
        // editable. A future association pass can flatten these without changing calibration.
        if !existing {
            app.run("select.set", json!({"ids":[b.id,group]}))?;
            let wrapper = app.run("object.group", json!({}))?.get("id").and_then(Value::as_u64).ok_or("缺少比例尺包装组合")?;
            app.run(
                "object.setProps",
                json!({"ids":[wrapper],"name":"科研比例尺组","data":{"sci-role":"scale-wrapper","sci-target":b.id.to_string()}}),
            )?;
        } else if let Some(parent) = parent {
            // Paste Into is not required: the document has a command for structural moves.
            app.run("layer.move", json!({"ids":[group],"target":parent.0,"place":"inside"}))?;
        }
    }
    Ok(group)
}

pub(super) fn read_metadata(app: &mut VectorcraftApp) -> Result<ScaleSettings, String> {
    let (boxes, nodes) = targets(app)?;
    let b = boxes.first().ok_or("请选择图片")?;
    let f = super::tiff::image_fov(app, b, &nodes)?;
    let mut s = app.ui.scientific.scale.clone();
    s.fov_unit = "um".into();
    s.fov_width = f.width.map(|n| n.to_string()).unwrap_or_default();
    s.fov_height = f.height.map(|n| n.to_string()).unwrap_or_default();
    Ok(s)
}

pub(super) fn load(app: &mut VectorcraftApp) -> Result<ScaleSettings, String> {
    let (boxes, _) = targets(app)?;
    let b = boxes.first().ok_or("请选择一张图片")?;
    let detail = app.run("document.node", json!({"id":b.id}))?;
    let mut s = if let Some(raw) = attribute(&detail, "sci-scale-options") {
        serde_json::from_str(raw).map_err(|_| "Invalid saved scale options")?
    } else {
        app.ui.scientific.scale.clone()
    };
    s.fov_width.clear();
    s.fov_height.clear();
    if let Some(f) = stored(app, b.id)? {
        let unit = factor(&s.fov_unit).unwrap_or(1.);
        s.fov_width = f.width.map(|v| (v / unit).to_string()).unwrap_or_default();
        s.fov_height = f.height.map(|v| (v / unit).to_string()).unwrap_or_default();
    } else if let Ok(loaded) = read_metadata(app) {
        s = loaded;
    }
    Ok(s)
}

#[derive(Clone)]
struct Pending {
    index: Option<usize>,
    title: String,
    selection: Vec<u64>,
    document: std::sync::Arc<vectorcraft_doc::Document>,
    settings: ScaleSettings,
    due: f64,
}

fn queue(app: &VectorcraftApp, ctx: &egui::Context, s: &ScaleSettings, manual: bool) {
    let current = app.session.active().map(|st| (format!("{:?}:{}:{:?}", app.session.active_index(), st.title(), st.selection.objects), st));
    let Some((key, st)) = current else { return };
    let old: Option<(String, ScaleSettings)> = crate::panels::pstate(ctx, "sci-scale-last-input");
    if manual || !s.auto_save {
        crate::panels::set_pstate(ctx, "sci-scale-save-pending", Option::<Pending>::None);
    } else if old.as_ref().is_some_and(|(old, params)| old == &key && params != s) {
        crate::panels::set_pstate(
            ctx,
            "sci-scale-save-pending",
            Some(Pending {
                index: app.session.active_index(),
                title: st.title(),
                selection: st.selection.objects.iter().map(|id| id.0).collect(),
                document: st.doc.clone(),
                settings: s.clone(),
                due: ctx.input(|i| i.time) + 0.5,
            }),
        );
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    crate::panels::set_pstate(ctx, "sci-scale-last-input", Some((key, s.clone())));
}

pub(crate) fn autosave_tick(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let pending: Option<Pending> = crate::panels::pstate(ctx, "sci-scale-save-pending");
    let Some(p) = pending else { return };
    if app.ui.dialog.is_some() || app.session.in_interaction() || ctx.input(|i| i.time) < p.due {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        return;
    }
    crate::panels::set_pstate(ctx, "sci-scale-save-pending", Option::<Pending>::None);
    let current = app.session.active();
    if p.index != app.session.active_index()
        || !current.is_some_and(|st| {
            st.title() == p.title
                && std::sync::Arc::ptr_eq(&st.doc, &p.document)
                && st.selection.objects.iter().map(|id| id.0).collect::<Vec<_>>() == p.selection
        })
    {
        app.status("选择或画稿变化，未自动写入先前参数");
        return;
    }
    fn has_bar(n: &vectorcraft_doc::Node, ids: &[u64]) -> bool {
        let own = n.extras().attrs.as_ref().is_some_and(|a| {
            a.data.iter().any(|(k, v)| k == "sci-role" && v == "scale")
                && a.data.iter().any(|(k, v)| k == "sci-target" && v.parse::<u64>().ok().is_some_and(|id| ids.contains(&id)))
        });
        own || n.children().is_some_and(|nodes| nodes.iter().any(|n| has_bar(n, ids)))
    }
    let result = (|| -> Result<(), String> {
        let (boxes, _) = targets(app)?;
        let ids: Vec<_> = boxes.iter().map(|b| b.id).collect();
        let bar = app.session.active().is_some_and(|st| st.doc.layers.iter().any(|n| has_bar(n, &ids)));
        app.run("ui.scientific", json!({"operation":if bar{"scale_apply"}else{"scale_calibrate"},"settings":{"scale":p.settings}}))?;
        Ok(())
    })();
    match result {
        Ok(()) => app.status("比例尺参数已自动保存"),
        Err(e) => app.status(format!("参数尚未保存：{e}")),
    }
}

pub(super) fn show(app: &mut VectorcraftApp, ui: &mut Ui, s: &mut Settings) {
    egui::CollapsingHeader::new("真实标定与比例尺").show(ui, |ui| {
        let mut manual = false;
        let key = app.session.active().map(|st| format!("{:?}:{}:{:?}", app.session.active_index(), st.title(), st.selection.objects));
        let old: Option<String> = crate::panels::pstate(ui.ctx(), "sci-scale-selection");
        if key != old {
            if let Ok(loaded) = load(app) {
                s.scale = loaded;
            }
            crate::panels::set_pstate(ui.ctx(), "sci-scale-selection", key);
        }
        ui.small("FOV 是所选区域的真实视野，不是打印 DPI。");
        for (name, value) in [("FOV 宽", &mut s.scale.fov_width), ("FOV 高", &mut s.scale.fov_height)] {
            ui.horizontal(|ui| {
                ui.label(name);
                ui.add(egui::TextEdit::singleline(value).desired_width(90.).hint_text("未知可留空"));
            });
        }
        units(ui, "sci-fov-unit", &mut s.scale.fov_unit);
        ui.checkbox(&mut s.scale.override_batch, "批量覆盖各图已有标定");
        ui.checkbox(&mut s.scale.auto_save, "停顿后自动保存标定 / 更新已有比例尺");
        ui.horizontal(|ui| {
            ui.label("代表长度（0隐藏）");
            ui.add(egui::DragValue::new(&mut s.scale.length).range(0.0..=1e12));
        });
        let old_unit = s.scale.unit.clone();
        units(ui, "sci-scale-unit", &mut s.scale.unit);
        if old_unit != s.scale.unit
            && let (Ok(old), Ok(new)) = (factor(&old_unit), factor(&s.scale.unit))
        {
            s.scale.length *= old / new;
        }
        ui.checkbox(&mut s.scale.vertical, "纵向比例尺");
        ui.horizontal(|ui| {
            ui.label("厚度 pt");
            ui.add(egui::DragValue::new(&mut s.scale.thickness).range(0.1..=100.));
        });
        ui.label("比例尺颜色");
        super::colors::field(app, ui, &mut s.scale.color, "scale");
        ui.checkbox(&mut s.scale.show_text, "显示文字");
        ui.horizontal(|ui| {
            ui.label("字号 pt");
            ui.add(egui::DragValue::new(&mut s.scale.font_size).range(1.0..=300.));
            ui.checkbox(&mut s.scale.bold, "粗体");
        });
        ui.label("文字颜色");
        super::colors::field(app, ui, &mut s.scale.font_color, "scale_text");
        egui::ComboBox::from_id_salt("sci-scale-corner").selected_text(&s.scale.corner).show_ui(ui, |ui| {
            for (value, name) in [("top-left", "左上"), ("top-right", "右上"), ("bottom-left", "左下"), ("bottom-right", "右下")] {
                ui.selectable_value(&mut s.scale.corner, value.into(), name);
            }
        });
        ui.horizontal(|ui| {
            ui.label("内缩 pt");
            ui.add(egui::DragValue::new(&mut s.scale.offset).range(0.0..=10000.));
        });
        ui.checkbox(&mut s.scale.auto_group, "与原图编组");
        for (op, name) in [
            ("scale_read_metadata", "读取链接 TIFF 标定"),
            ("scale_load", "读取所选标定"),
            ("scale_calibrate", "仅保存标定"),
            ("scale_apply", "添加 / 更新比例尺"),
        ] {
            if ui.button(name).clicked() {
                match app.run("ui.scientific", json!({"operation":op,"settings":s})) {
                    Ok(_) => {
                        *s = app.ui.scientific.clone();
                        manual = op != "scale_read_metadata";
                        app.status("比例尺操作完成");
                    }
                    Err(e) => app.status(e),
                }
            }
        }
        queue(app, ui.ctx(), &s.scale, manual);
    });
}

fn units(ui: &mut Ui, id: &str, value: &mut String) {
    egui::ComboBox::from_id_salt(id).selected_text(value.as_str()).show_ui(ui, |ui| {
        for unit in ["nm", "um", "mm", "cm", "m", "inch"] {
            ui.selectable_value(value, unit.into(), unit);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_length_uses_real_fov_not_document_dpi() {
        let b = BoxInfo { id: 1, x: 0., y: 0., w: 200., h: 100. };
        let f = Fov { width: Some(1000.), height: None };
        let mut s = ScaleSettings { length: 0.2, unit: "mm".into(), ..Default::default() };
        assert_eq!(physical_length(&s, &f, &b).unwrap(), 40.);
        s.vertical = true;
        assert!(physical_length(&s, &f, &b).is_err());
        s.vertical = false;
        s.length = 2.;
        assert!(physical_length(&s, &f, &b).is_err());
    }
    #[test]
    fn scale_updates_are_grouped_and_invalid_lengths_leave_no_edits() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({})).unwrap();
        let id = app.run("shape.rectangle", json!({"x":20,"y":30,"width":200,"height":100})).unwrap()["id"].as_u64().unwrap();
        app.run("select.set", json!({"ids":[id]})).unwrap();
        app.run("ui.scientific", json!({"operation":"scale_apply","settings":{"scale":{"fov_width":"1000","length":200.}}})).unwrap();
        let first = app.run("document.inspect", json!({})).unwrap();
        app.run("ui.scientific", json!({"operation":"scale_apply","settings":{"scale":{"length":100.}}})).unwrap();
        let second = app.run("document.inspect", json!({})).unwrap();
        fn count(n: &Value) -> usize {
            usize::from(n.get("name").and_then(Value::as_str).unwrap_or("").starts_with("[SCI scale]"))
                + n.get("children").and_then(Value::as_array).into_iter().flatten().map(count).sum::<usize>()
        }
        assert_eq!(second["layers"].as_array().unwrap().iter().map(count).sum::<usize>(), 1);
        assert!(app.run("ui.scientific", json!({"operation":"scale_apply","settings":{"scale":{"length":2000.}}})).is_err());
        assert_eq!(app.run("document.inspect", json!({})).unwrap(), second);
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(app.run("document.inspect", json!({})).unwrap()["layers"], first["layers"]);
    }
}
