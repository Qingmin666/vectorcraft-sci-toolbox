//! Editable RGB component masks and Screen channel overlays. No source pixel files are written.
use super::*;
use vectorcraft_doc::{Node, NodeId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerSettings {
    pub lut: String,
    pub keep_original: bool,
    pub gap: f64,
}
impl Default for LayerSettings {
    fn default() -> Self {
        Self { lut: "red".into(), keep_original: false, gap: 18. }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Parts {
    version: u8,
    raw: u64,
    tint: u64,
    background: u64,
    lut: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub id: u64,
    pub name: String,
    pub lut: String,
    pub enabled: bool,
}
#[derive(Clone, Debug)]
pub struct ChannelSession {
    document: std::sync::Arc<vectorcraft_doc::Document>,
    index: Option<usize>,
    ids: Vec<u64>,
}
#[derive(Clone)]
struct Entry {
    b: BoxInfo,
    raw: u64,
    parts: Option<Parts>,
    fov: Option<scale::Fov>,
    scale: Option<scale::ScaleSettings>,
}

fn color(lut: &str) -> Result<&'static str, String> {
    match lut {
        "red" => Ok("#FF0000"),
        "green" => Ok("#00FF00"),
        "blue" => Ok("#0000FF"),
        "cyan" => Ok("#00FFFF"),
        "magenta" => Ok("#FF00FF"),
        "yellow" => Ok("#FFFF00"),
        "gray" | "grays" => Ok("#FFFFFF"),
        _ => Err("未知的 RGB 分量颜色".into()),
    }
}
fn id(value: &Value) -> Result<u64, String> {
    value.get("id").and_then(Value::as_u64).ok_or("缺少新通道对象".into())
}
fn image_count(n: &Node) -> usize {
    usize::from(matches!(n.kind, vectorcraft_doc::NodeKind::Image(_)))
        + n.children().map(|nodes| nodes.iter().map(|n| image_count(n)).sum::<usize>()).unwrap_or(0)
}

fn entries(app: &mut VectorcraftApp) -> Result<Vec<Entry>, String> {
    let (boxes, _) = scale::targets(app)?;
    let mut entries = Vec::new();
    for mut b in boxes {
        let mut cursor = b.id;
        for _ in 0..64 {
            let parent = app.session.active().and_then(|st| st.doc.parent_of(NodeId(cursor)));
            let Some(parent) = parent else { break };
            let detail = app.run("document.node", json!({"id":parent.0}))?;
            if attribute(&detail, "sci-pseudocolor").is_some() {
                b = bounds(&app.run("document.node", json!({"id":parent.0,"summary":true}))?)?;
                break;
            }
            cursor = parent.0;
        }
        if entries.iter().any(|e: &Entry| e.b.id == b.id) {
            continue;
        }

        let detail = app.run("document.node", json!({"id":b.id}))?;
        let parts: Option<Parts> = attribute(&detail, "sci-pseudocolor")
            .map(|s| serde_json::from_str(s).map_err(|_| "Invalid pseudocolor group".to_string()))
            .transpose()?;
        let st = app.session.active().ok_or("没有打开文档")?;
        let root = st.doc.node(NodeId(b.id)).ok_or("找不到图片")?;
        if root.opacity != 1.0
            || (!matches!(root.blend, vectorcraft_color::BlendMode::Normal) && parts.is_none())
            || parts.is_none() && root.bbox_angle.abs() > 1e-6
        {
            return Err("请使用未旋转、100%不透明且正常混合的图片；完成后再旋转结果".into());
        }
        let raw = parts.as_ref().map(|p| p.raw).unwrap_or(b.id);
        let node = st.doc.node(NodeId(raw)).ok_or("原通道像素对象已被移除")?;
        if image_count(node) != 1 {
            return Err("请选择单张图片、裁剪组合或单个伪彩通道".into());
        }
        if let Some(parts) = &parts {
            let owner = st.doc.index_path(NodeId(b.id)).ok_or("找不到通道")?;
            for child in [parts.raw, parts.tint, parts.background] {
                let path = st.doc.index_path(NodeId(child)).ok_or("缺少通道组成对象")?;
                if child == b.id || !path.starts_with(&owner) {
                    return Err("通道组成对象超出所属组合".into());
                }
            }
        }
        let fov = scale::stored(app, b.id)?;
        let options = attribute(&detail, "sci-scale-options")
            .map(|raw| serde_json::from_str(raw).map_err(|_| "Invalid scale options".to_string()))
            .transpose()?;
        entries.push(Entry { b, raw, parts, fov, scale: options });
    }
    Ok(entries)
}

fn mask_group(app: &mut VectorcraftApp, e: &Entry, lut: &str, x: f64, y: f64, move_original: bool) -> Result<u64, String> {
    let tint_color = color(lut)?;
    let bg = id(&app.run("shape.rectangle", json!({"x":x,"y":y,"width":e.b.w,"height":e.b.h}))?)?;
    app.run("paint.setFill", json!({"ids":[bg],"color":"#000000"}))?;
    app.run("paint.setStroke", json!({"ids":[bg],"none":true}))?;
    let raw = if move_original {
        app.run("layer.move", json!({"ids":[bg],"target":e.raw,"place":"below"}))?;
        e.raw
    } else {
        let made = app.run("object.transform", json!({"ids":[e.raw],"matrix":[1,0,0,1,x-e.b.x,y-e.b.y],"copy":true}))?;
        let raw = made.get("ids").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_u64).ok_or("缺少通道副本")?;
        super::zoom::sanitize_copy(app, raw)?;
        app.run("layer.move", json!({"ids":[raw],"target":bg,"place":"above"}))?;
        raw
    };
    let tint = id(&app.run("shape.rectangle", json!({"x":x,"y":y,"width":e.b.w,"height":e.b.h}))?)?;
    app.run("paint.setFill", json!({"ids":[tint],"color":tint_color}))?;
    app.run("paint.setStroke", json!({"ids":[tint],"none":true}))?;
    app.run("object.setProps", json!({"ids":[tint],"blend":"darken"}))?;
    app.run("layer.move", json!({"ids":[tint],"target":raw,"place":"above"}))?;
    app.run("select.set", json!({"ids":[bg,raw,tint]}))?;
    let root = id(&app.run("object.group", json!({}))?)?;
    let parts = Parts { version: 1, raw, tint, background: bg, lut: lut.into() };
    let mut data = json!({"sci-role":"pseudocolor","sci-pseudocolor":serde_json::to_string(&parts).map_err(|e|e.to_string())?});
    if let Some(fov) = &e.fov {
        data["sci-fov"] = json!(serde_json::to_string(fov).map_err(|e| e.to_string())?);
    }
    if let Some(options) = &e.scale {
        data["sci-scale-options"] = json!(serde_json::to_string(options).map_err(|e| e.to_string())?);
    }
    app.run("object.setProps", json!({"ids":[root],"name":format!("科研伪彩 {lut}"),"isolate":true,"data":data}))?;
    if move_original {
        super::zoom::repoint_source(app, e.b.id, root)?;
    }
    Ok(root)
}

pub(super) fn batch(app: &mut VectorcraftApp, s: &LayerSettings) -> Result<(), String> {
    color(&s.lut)?;
    if !s.gap.is_finite() || !(0.0..=10000.).contains(&s.gap) {
        return Err("复制间隔无效".into());
    }
    let entries = entries(app)?;
    let mut results = Vec::new();
    transaction(app, "可编辑图片伪彩", entries.iter().map(|e| e.b.id).collect(), |app| {
        for e in &entries {
            if let Some(parts) = &e.parts {
                let mut parts = parts.clone();
                parts.lut = s.lut.clone();
                app.run("paint.setFill", json!({"ids":[parts.tint],"color":color(&s.lut)?}))?;
                app.run(
                    "object.setProps",
                    json!({"ids":[e.b.id],"data":{"sci-pseudocolor":serde_json::to_string(&parts).map_err(|e|e.to_string())?}}),
                )?;
                results.push(e.b.id);
            } else {
                let root = mask_group(app, e, &s.lut, if s.keep_original { e.b.x + e.b.w + s.gap } else { e.b.x }, e.b.y, !s.keep_original)?;
                if s.keep_original
                    && let (Some(fov), Some(options)) = (&e.fov, &e.scale)
                    && options.length > 0.
                {
                    let b = bounds(&app.run("document.node", json!({"id":root,"summary":true}))?)?;
                    scale::draw(app, &b, fov, options)?;
                }
                results.push(root);
            }
        }
        Ok(())
    })?;
    app.run("select.set", json!({"ids":results}))?;
    Ok(())
}

pub(super) fn read_channels(app: &mut VectorcraftApp) -> Result<Value, String> {
    let entries = entries(app)?;
    if !(2..=7).contains(&entries.len()) {
        return Err("请选择2至7个通道".into());
    }
    let defaults = ["red", "green", "blue", "cyan", "magenta", "yellow", "gray"];
    let rows: Vec<_> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| Channel {
            id: e.b.id,
            name: format!("通道 {} · #{}", i + 1, e.b.id),
            lut: e.parts.as_ref().map(|p| p.lut.clone()).unwrap_or_else(|| defaults.get(i).copied().unwrap_or("gray").into()),
            enabled: true,
        })
        .collect();
    let st = app.session.active().ok_or("没有打开文档")?;
    app.ui.scientific_channel_session =
        Some(ChannelSession { document: st.doc.clone(), index: app.session.active_index(), ids: entries.iter().map(|e| e.b.id).collect() });
    app.ui.scientific_channels = rows;
    serde_json::to_value(&app.ui.scientific_channels).map_err(|e| e.to_string())
}

pub(super) fn configure_channels(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let rows: Vec<Channel> = serde_json::from_value(p.get("channels").cloned().ok_or("缺少通道参数")?).map_err(|e| e.to_string())?;
    let session = app.ui.scientific_channel_session.as_ref().ok_or("请先读取所选通道")?;
    if rows.len() != session.ids.len()
        || rows.iter().any(|r| !session.ids.contains(&r.id))
        || rows.iter().map(|r| r.id).collect::<std::collections::BTreeSet<_>>().len() != rows.len()
    {
        return Err("选择已改变，请重新读取通道".into());
    }
    for r in &rows {
        color(&r.lut)?;
        if r.name.len() > 120 {
            return Err("通道名称过长".into());
        }
    }
    app.ui.scientific_channels = rows;
    Ok(json!({"ok":true}))
}

pub(super) fn merge(app: &mut VectorcraftApp) -> Result<(), String> {
    let entries = entries(app)?;
    if app.ui.scientific_channels.is_empty() {
        read_channels(app)?;
    }
    let session = app.ui.scientific_channel_session.as_ref().ok_or("请先读取通道")?;
    let st = app.session.active().ok_or("没有打开文档")?;
    if session.index != app.session.active_index()
        || !std::sync::Arc::ptr_eq(&session.document, &st.doc)
        || session.ids != entries.iter().map(|e| e.b.id).collect::<Vec<_>>()
    {
        return Err("画稿已改变，请重新读取所选通道".into());
    }
    let enabled: Vec<_> = app.ui.scientific_channels.iter().filter(|c| c.enabled).cloned().collect();
    if !(2..=7).contains(&enabled.len()) {
        return Err("请启用2至7个通道".into());
    }
    let chosen = enabled
        .iter()
        .map(|c| entries.iter().find(|e| e.b.id == c.id).cloned().map(|e| (c.clone(), e)).ok_or("通道源图已被移除"))
        .collect::<Result<Vec<_>, _>>()?;
    let first = &chosen.first().ok_or("没有通道")?.1;
    for (_, e) in &chosen {
        if st.doc.node(vectorcraft_doc::NodeId(e.b.id)).ok_or("通道已被移除")?.bbox_angle.abs() > 1e-6 {
            return Err("请先合并通道再旋转结果；本操作不自动配准旋转通道".into());
        }
        if (e.b.w - first.b.w).abs() > 0.001 || (e.b.h - first.b.h).abs() > 0.001 {
            return Err("通道显示尺寸必须相同；本操作不自动配准图像".into());
        }
    }
    let x = chosen.iter().map(|(_, e)| e.b.x + e.b.w).fold(f64::NEG_INFINITY, f64::max) + 18.;
    let y = first.b.y;
    let mut merged = 0;
    transaction(app, "Screen 合并通道", entries.iter().map(|e| e.b.id).collect(), |app| {
        let mut roots = Vec::new();
        for (i, (c, e)) in chosen.iter().enumerate() {
            let root = mask_group(app, e, &c.lut, x, y, false)?;
            app.run("object.setProps", json!({"ids":[root],"blend":if i==0{"normal"}else{"screen"}}))?;
            roots.push(root);
        }
        app.run("select.set", json!({"ids":roots}))?;
        merged = id(&app.run("object.group", json!({}))?)?;
        app.run("object.setProps", json!({"ids":[merged],"name":"科研通道合并 · Screen","isolate":true,"data":{"sci-role":"merged-channels"}}))?;
        if chosen.iter().all(|(_, e)| e.fov.is_some()) {
            let fov = first.fov.as_ref().ok_or("缺少实际标定")?;
            let same = |a: Option<f64>, b: Option<f64>| match (a, b) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-6 * a.abs().max(1.),
                (None, None) => true,
                _ => false,
            };
            if chosen.iter().any(|(_, e)| e.fov.as_ref().is_some_and(|f| !same(f.width, fov.width) || !same(f.height, fov.height))) {
                return Err("通道的真实标定不一致".into());
            }
            app.run("object.setProps", json!({"ids":[merged],"data":{"sci-fov":serde_json::to_string(fov).map_err(|e|e.to_string())?}}))?;
            if let Some(options) = &first.scale {
                app.run(
                    "object.setProps",
                    json!({"ids":[merged],"data":{"sci-scale-options":serde_json::to_string(options).map_err(|e|e.to_string())?}}),
                )?;
                let b = bounds(&app.run("document.node", json!({"id":merged,"summary":true}))?)?;
                if options.length > 0. {
                    scale::draw(app, &b, fov, options)?;
                }
            }
        }
        Ok(())
    })?;
    app.ui.scientific_channel_session = None;
    app.ui.scientific_channels.clear();
    app.run("select.set", json!({"ids":[merged]}))?;
    Ok(())
}

fn choose(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut String) {
    egui::ComboBox::from_id_salt(id).selected_text(value.as_str()).show_ui(ui, |ui| {
        for (lut, name) in
            [("red", "红"), ("green", "绿"), ("blue", "蓝"), ("cyan", "青"), ("magenta", "品红"), ("yellow", "黄"), ("gray", "灰度 / 原分量")]
        {
            ui.horizontal(|ui| {
                if let Ok(hex) = color(lut)
                    && let Some(c) = crate::panels::color::parse_hex(hex)
                {
                    let [r, g, b, _] = c.to_rgba8(1.);
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(60., 14.), egui::Sense::hover());
                    for i in 0..24 {
                        let t = i as f32 / 23.;
                        let part = egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * 2.5, 0.), egui::vec2(2.5, 14.));
                        ui.painter().rect_filled(
                            part,
                            0.,
                            egui::Color32::from_rgb((f32::from(r) * t) as u8, (f32::from(g) * t) as u8, (f32::from(b) * t) as u8),
                        );
                    }
                }
                ui.selectable_value(value, lut.into(), name);
            });
        }
    });
}
pub(super) fn show(app: &mut VectorcraftApp, ui: &mut Ui, s: &mut Settings) {
    egui::CollapsingHeader::new("可编辑伪彩与通道合并").show(ui, |ui| {
        ui.small("RGB 分量着色；透明区域显示黑色。不会改写源图片文件。");
        choose(ui, "sci-pseudo-lut", &mut s.pseudocolor.lut);
        ui.checkbox(&mut s.pseudocolor.keep_original, "保留原图，在右侧创建副本");
        if ui.button("应用 / 更新伪彩").clicked() {
            match app.run("ui.scientific", json!({"operation":"pseudocolor_apply","settings":s})) {
                Ok(_) => app.status("伪彩已更新"),
                Err(e) => app.status(e),
            }
        }
        ui.separator();
        ui.small("Screen 叠加 2–7 个显示尺寸相同的通道，不做像素配准或定量加法。");
        if ui.button("读取所选通道").clicked()
            && let Err(e) = app.run("ui.scientific", json!({"operation":"channels_read"}))
        {
            app.status(e);
        }
        for (i, c) in app.ui.scientific_channels.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut c.enabled, &c.name);
                choose(ui, ("sci-channel", i), &mut c.lut);
            });
        }
        if ui.button("Screen 合并启用通道").clicked() {
            match app.run("ui.scientific", json!({"operation":"channels_merge"})) {
                Ok(_) => app.status("通道已合并，原图保留"),
                Err(e) => app.status(e),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_testkit::raster::{assert_rgb, render_view};
    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width":200,"height":100})).unwrap();
        app
    }
    fn image(app: &mut VectorcraftApp, x: f64, gray: u8) -> u64 {
        let image = image::RgbaImage::from_pixel(16, 16, image::Rgba([gray, gray, gray, 255]));
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut output, image::ImageFormat::Png).unwrap();
        app.run(
            "file.place",
            json!({"name":"synthetic.png","dataBase64":vectorcraft_format::base64_encode(&output.into_inner()),"at":[x,10],"link":false}),
        )
        .unwrap()["ids"][0]
            .as_u64()
            .unwrap()
    }
    #[test]
    fn pseudocolor_is_editable_and_preserves_original_pixel_node() {
        let mut app = app();
        let source = image(&mut app, 10., 128);
        app.run("select.set", json!({"ids":[source]})).unwrap();
        app.run("ui.scientific", json!({"operation":"pseudocolor_apply"})).unwrap();
        let root = app.session.active().unwrap().selection.objects[0].0;
        assert!(app.session.active().unwrap().doc.node(NodeId(source)).is_some());
        assert_rgb(&render_view(&app.session.active().unwrap().doc, 200, 100), 10, 10, [128, 0, 0], 3);
        app.run("ui.scientific", json!({"operation":"pseudocolor_apply","settings":{"pseudocolor":{"lut":"green"}}})).unwrap();
        assert_eq!(app.session.active().unwrap().selection.objects[0].0, root);
        assert_rgb(&render_view(&app.session.active().unwrap().doc, 200, 100), 10, 10, [0, 128, 0], 3);
        app.run("edit.undo", json!({})).unwrap();
        assert_rgb(&render_view(&app.session.active().unwrap().doc, 200, 100), 10, 10, [128, 0, 0], 3);
    }
    #[test]
    fn screen_merge_keeps_sources_and_is_not_saturating_addition() {
        let mut app = app();
        let a = image(&mut app, 10., 128);
        let b = image(&mut app, 40., 128);
        app.run("select.set", json!({"ids":[a,b]})).unwrap();
        read_channels(&mut app).unwrap();
        let mut rows = app.ui.scientific_channels.clone();
        for row in &mut rows {
            row.lut = "red".into();
        }
        configure_channels(&mut app, &json!({"channels":rows})).unwrap();
        merge(&mut app).unwrap();
        let doc = &app.session.active().unwrap().doc;
        assert!(doc.node(NodeId(a)).is_some() && doc.node(NodeId(b)).is_some());
        let image = render_view(doc, 200, 100);
        assert_rgb(&image, 10, 10, [128, 128, 128], 3);
        assert_rgb(&image, 74, 10, [192, 0, 0], 4);
    }
    #[test]
    fn invalid_merge_keeps_the_entire_document() {
        let mut app = app();
        let a = image(&mut app, 10., 64);
        let b = image(&mut app, 40., 64);
        app.run("select.set", json!({"ids":[a,b]})).unwrap();
        read_channels(&mut app).unwrap();
        app.ui.scientific_channels[0].enabled = false;
        let before = app.run("document.inspect", json!({})).unwrap();
        assert!(merge(&mut app).is_err());
        assert_eq!(app.run("document.inspect", json!({})).unwrap(), before);
    }
}
