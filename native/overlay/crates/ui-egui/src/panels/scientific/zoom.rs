//! Editable ROI crops, persistent associations and transaction-aware synchronization.
use super::*;
use std::hash::{Hash, Hasher};
use vectorcraft_doc::{Node, NodeId, NodeKind};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Region {
    pub key: u64,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub color: String,
    pub scale_length: String,
    pub scale_unit: String,
}
impl Default for Region {
    fn default() -> Self {
        Self {
            key: 0,
            name: "ROI 1".into(),
            x: 0.2,
            y: 0.2,
            width: 0.3,
            height: 0.3,
            rotation: 0.,
            color: String::new(),
            scale_length: String::new(),
            scale_unit: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ZoomSettings {
    pub regions: Vec<Region>,
    pub placement: String,
    pub magnification: f64,
    pub gap: f64,
    pub color: String,
    pub stroke: f64,
    pub dash: String,
    pub guides: bool,
    pub extent: String,
    pub auto_update: bool,
    pub inherit_scale: bool,
    pub use_rectangle_color: bool,
}
impl Default for ZoomSettings {
    fn default() -> Self {
        Self {
            regions: vec![Region::default()],
            placement: "right".into(),
            magnification: 0.,
            gap: 18.,
            color: "#FF6B35".into(),
            stroke: 1.,
            dash: "dash".into(),
            guides: true,
            extent: "across".into(),
            auto_update: true,
            inherit_scale: true,
            use_rectangle_color: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Record {
    region: Region,
    source: u64,
    marker: u64,
    output: u64,
    guides: Vec<u64>,
    source_frame: [f64; 5],
    marker_frame: [f64; 5],
    output_frame: [f64; 5],
    signature: u64,
    style: ZoomSettings,
}

#[derive(Clone, Debug)]
pub struct Tracker {
    index: Option<usize>,
    title: String,
    revision: u64,
    document: std::sync::Arc<vectorcraft_doc::Document>,
}

fn id(value: &Value) -> Result<u64, String> {
    value.get("id").and_then(Value::as_u64).ok_or("缺少新对象编号".into())
}
fn frame(app: &VectorcraftApp, id: u64) -> Result<[f64; 5], String> {
    let st = app.session.active().ok_or("没有打开文档")?;
    let node = st.doc.node(NodeId(id)).ok_or("对象已被移除")?;
    let output = node.extras().attrs.as_ref().is_some_and(|a| a.data.iter().any(|(k, v)| k == "sci-role" && v == "zoom-output"));
    let bounds_id = if output {
        node.children()
            .and_then(|nodes| nodes.iter().find(|n| matches!(n.kind, NodeKind::Group { clip: true, .. })))
            .map(|n| n.id)
            .unwrap_or(NodeId(id))
    } else {
        NodeId(id)
    };
    let b = st.doc.oriented_bounds(&[bounds_id], false).ok_or("找不到对象边界")?;
    let c = b.center();
    Ok([c.x, c.y, b.rect.width(), b.rect.height(), b.angle])
}
fn changed(a: &[f64; 5], b: &[f64; 5]) -> bool {
    a.iter().zip(b).any(|(a, b)| (a - b).abs() > 1e-5)
}
fn box_of(id: u64, f: &[f64; 5]) -> BoxInfo {
    BoxInfo { id, x: f[0] - f[2] / 2., y: f[1] - f[3] / 2., w: f[2], h: f[3] }
}
fn source_signature(n: &Node, h: &mut impl Hasher) {
    n.opacity.to_bits().hash(h);
    format!("{:?}", n.blend).hash(h);
    n.isolate.hash(h);
    if let NodeKind::Image(i) = &n.kind {
        i.key.hash(h);
        i.width.hash(h);
        i.height.hash(h);
        for c in i.xf.as_coeffs() {
            c.to_bits().hash(h);
        }
    }
    // Pseudocolor tint changes are appearance changes, even if source pixel keys are unchanged.
    if let Ok(value) = serde_json::to_string(&n.appearance) {
        value.hash(h);
    }
    if let Some(children) = n.children() {
        for child in children {
            source_signature(child, h);
        }
    }
}
fn signature(app: &VectorcraftApp, id: u64) -> Result<u64, String> {
    let st = app.session.active().ok_or("没有打开文档")?;
    let node = st.doc.node(NodeId(id)).ok_or("找不到源对象")?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source_signature(node, &mut h);
    if let Some(attrs) = node.extras().attrs.as_ref() {
        for (k, v) in &attrs.data {
            if k == "sci-fov" || k == "sci-scale-options" {
                v.hash(&mut h);
            }
        }
    }
    Ok(h.finish())
}
fn raw_records(n: &Node) -> Option<&str> {
    n.extras().attrs.as_ref()?.data.iter().find(|(k, _)| k == "sci-zoom-records").map(|(_, v)| v.as_str())
}
fn check_record(app: &VectorcraftApp, source: u64, r: &Record) -> Result<(), String> {
    if source != r.source || r.marker == source || r.output == source || r.marker == r.output || r.guides.len() > 2 {
        return Err("ROI 关联对象编号无效".into());
    }
    let st = app.session.active().ok_or("没有打开文档")?;
    for (id, role) in std::iter::once((r.marker, "zoom-marker"))
        .chain(std::iter::once((r.output, "zoom-output")))
        .chain(r.guides.iter().map(|id| (*id, "zoom-guide")))
    {
        let Some(node) = st.doc.node(NodeId(id)) else { continue };
        let attrs = node.extras().attrs.as_ref().ok_or("ROI 关联标签丢失")?;
        let read = |key| attrs.data.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        if read("sci-role") != Some(role) || read("sci-source") != Some(source.to_string().as_str()) {
            return Err("ROI 关联指向不相关的画稿".into());
        }
        let path = st.doc.index_path(NodeId(id)).ok_or("找不到 ROI 对象")?;
        for end in 1..=path.len() {
            if st.doc.node_at(path.get(..end).ok_or("ROI 路径无效")?).is_some_and(|n| n.locked || !n.visible) {
                return Err("请先解锁并显示关联的 ROI 对象".into());
            }
        }
    }
    Ok(())
}

fn records(app: &VectorcraftApp, id: u64) -> Result<Vec<Record>, String> {
    let node = app.session.active().and_then(|st| st.doc.node(NodeId(id))).ok_or("找不到源对象")?;
    let records: Vec<Record> =
        raw_records(node).map(|s| serde_json::from_str(s).map_err(|_| "Invalid ROI associations".to_string())).unwrap_or_else(|| Ok(Vec::new()))?;
    if records.len() > 32 {
        return Err("ROI 关联数量过多".into());
    }
    for r in &records {
        check_record(app, id, r)?;
    }
    Ok(records)
}

fn save(app: &mut VectorcraftApp, id: u64, records: &[Record]) -> Result<(), String> {
    app.run("object.setProps", json!({"ids":[id],"data":{"sci-zoom-records":serde_json::to_string(records).map_err(|e|e.to_string())?}}))?;
    Ok(())
}
fn has_image(n: &Value) -> bool {
    n.get("kind").and_then(Value::as_str) == Some("Image") || n.get("children").and_then(Value::as_array).into_iter().flatten().any(has_image)
}
fn sources(app: &mut VectorcraftApp) -> Result<(Vec<BoxInfo>, BTreeMap<u64, Value>), String> {
    let (mut boxes, nodes) = selection(app)?;
    for b in &mut boxes {
        let d = app.run("document.node", json!({"id":b.id}))?;
        if matches!(attribute(&d, "sci-role"), Some("scale-wrapper" | "border-wrapper")) {
            let id = attribute(&d, "sci-target").and_then(|s| s.parse::<u64>().ok()).ok_or("标注组合的关联已损坏")?;
            *b = bounds(nodes.get(&id).ok_or("找不到源对象")?)?;
        }
        let d = app.run("document.node", json!({"id":b.id}))?;
        if attribute(&d, "sci-role") == Some("zoom-output") {
            return Err("请对原始源图创建关联 ROI".into());
        }
    }
    let boxes: Vec<_> = boxes.into_iter().filter(|b| nodes.get(&b.id).is_some_and(has_image)).collect();
    if boxes.is_empty() {
        return Err("请选择源图片或图片组合".into());
    }
    Ok((boxes, nodes))
}
fn validate(r: &Region, s: &ZoomSettings) -> Result<(), String> {
    if r.name.len() > 120
        || [r.x, r.y, r.width, r.height, r.rotation, s.magnification, s.gap, s.stroke].iter().any(|n| !n.is_finite())
        || r.x < 0.
        || r.y < 0.
        || r.width <= 0.
        || r.height <= 0.
        || r.x + r.width > 1.000001
        || r.y + r.height > 1.000001
        || !(0.0..=100.).contains(&s.magnification)
        || !(0.0..=10000.).contains(&s.gap)
        || !(0.01..=100.).contains(&s.stroke)
        || !["right", "left", "top", "bottom"].contains(&s.placement.as_str())
        || !["inside", "across"].contains(&s.extent.as_str())
    {
        return Err("ROI 几何、位置或样式无效".into());
    }
    extended::valid_color(&s.color)?;
    dash(&s.dash, s.stroke)?;
    Ok(())
}
fn dash(name: &str, w: f64) -> Result<Value, String> {
    Ok(match name {
        "solid" => Value::Null,
        "dash" => json!([w * 4., w * 3.]),
        "dot" => json!([0., w * 3.]),
        "dashdot" => json!([w * 5., w * 3., 0., w * 3.]),
        "dashdotdot" => json!([w * 5., w * 3., 0., w * 3., 0., w * 3.]),
        "original" => return Ok(Value::Null),
        _ => return Err("未知的 ROI 线型".into()),
    })
}
fn marker_frame(source: &[f64; 5], r: &Region) -> Result<[f64; 5], String> {
    let angle = -source[4].to_radians();
    let (sin, cos) = angle.sin_cos();
    let dx = (r.x + r.width / 2. - 0.5) * source[2];
    let dy = (r.y + r.height / 2. - 0.5) * source[3];
    let half_x = (r.rotation.to_radians().cos().abs() * r.width * source[2] + r.rotation.to_radians().sin().abs() * r.height * source[3]) / 2.;
    let half_y = (r.rotation.to_radians().sin().abs() * r.width * source[2] + r.rotation.to_radians().cos().abs() * r.height * source[3]) / 2.;
    if dx - half_x < -source[2] / 2. - 1e-6
        || dx + half_x > source[2] / 2. + 1e-6
        || dy - half_y < -source[3] / 2. - 1e-6
        || dy + half_y > source[3] / 2. + 1e-6
    {
        return Err("旋转后的 ROI 超出了源图".into());
    }
    Ok([source[0] + cos * dx - sin * dy, source[1] + sin * dx + cos * dy, r.width * source[2], r.height * source[3], source[4] + r.rotation])
}
fn normalized(source: &[f64; 5], marker: &[f64; 5], previous: &Region) -> Region {
    let angle = source[4].to_radians();
    let (sin, cos) = angle.sin_cos();
    let dx = marker[0] - source[0];
    let dy = marker[1] - source[1];
    Region {
        x: 0.5 + (cos * dx - sin * dy - marker[2] / 2.) / source[2],
        y: 0.5 + (sin * dx + cos * dy - marker[3] / 2.) / source[3],
        width: marker[2] / source[2],
        height: marker[3] / source[3],
        rotation: marker[4] - source[4],
        ..previous.clone()
    }
}
fn initial_output(b: &BoxInfo, source: &[f64; 5], r: &Region, s: &ZoomSettings, index: usize) -> [f64; 5] {
    let ratio = r.width * source[2] / (r.height * source[3]);
    let (w, h) = if s.magnification > 0. {
        (r.width * source[2] * s.magnification, r.height * source[3] * s.magnification)
    } else if s.placement == "right" || s.placement == "left" {
        (b.h * ratio, b.h)
    } else {
        (b.w, b.w / ratio)
    };
    let (x, y) = match s.placement.as_str() {
        "left" => (b.x - w - s.gap, b.y + index as f64 * (h + s.gap)),
        "top" => (b.x + index as f64 * (w + s.gap), b.y - h - s.gap),
        "bottom" => (b.x + index as f64 * (w + s.gap), b.y + b.h + s.gap),
        _ => (b.x + b.w + s.gap, b.y + index as f64 * (h + s.gap)),
    };
    [x + w / 2., y + h / 2., w, h, 0.]
}
fn rotate(app: &mut VectorcraftApp, id: u64, angle: f64, cx: f64, cy: f64) -> Result<(), String> {
    if angle.abs() > 1e-8 {
        app.run("select.set", json!({"ids":[id]}))?;
        app.run("object.rotate", json!({"angle":angle,"origin":[cx,cy]}))?;
    }
    Ok(())
}
fn set_marker(app: &mut VectorcraftApp, id: u64, desired: &[f64; 5]) -> Result<(), String> {
    let current = frame(app, id)?;
    rotate(app, id, -current[4], current[0], current[1])?;
    let current = frame(app, id)?;
    let sx = desired[2] / current[2];
    let sy = desired[3] / current[3];
    app.run("object.transform", json!({"ids":[id],"matrix":[sx,0,0,sy,desired[0]-sx*current[0],desired[1]-sy*current[1]],"strokes":false}))?;
    rotate(app, id, desired[4], desired[0], desired[1])
}
fn stroke(app: &mut VectorcraftApp, id: u64, s: &ZoomSettings) -> Result<(), String> {
    app.run("paint.setFill", json!({"ids":[id],"none":true}))?;
    app.run("paint.setStroke", json!({"ids":[id],"color":s.color}))?;
    if s.dash != "original" {
        app.run(
            "stroke.set",
            json!({"ids":[id],"weight":s.stroke,"dash":dash(&s.dash,s.stroke)?,"cap":if s.dash.contains("dot"){"round"}else{"butt"}}),
        )?;
    }
    Ok(())
}
fn children(app: &VectorcraftApp, id: u64) -> Vec<u64> {
    app.session
        .active()
        .and_then(|st| st.doc.node(NodeId(id)))
        .and_then(Node::children)
        .map(|nodes| nodes.iter().map(|n| n.id.0).collect())
        .unwrap_or_default()
}
fn clear(app: &mut VectorcraftApp, ids: &[u64]) -> Result<(), String> {
    let ids: Vec<_> = ids.iter().copied().filter(|id| app.session.active().is_some_and(|st| st.doc.node(NodeId(*id)).is_some())).collect();
    if !ids.is_empty() {
        app.run("edit.clear", json!({"ids":ids}))?;
    }
    Ok(())
}

pub(super) fn sanitize_copy(app: &mut VectorcraftApp, id: u64) -> Result<(), String> {
    fn collect(n: &Node, out: &mut Vec<u64>) {
        if n.extras().attrs.as_ref().is_some_and(|a| {
            a.data.iter().any(|(k, _)| {
                matches!(
                    k.as_str(),
                    "sci-zoom-records" | "sci-target" | "sci-source" | "sci-key" | "sci-role" | "sci-scale-options" | "sci-pseudocolor"
                )
            })
        }) {
            out.push(n.id.0);
        }
        if let Some(children) = n.children() {
            for n in children {
                collect(n, out);
            }
        }
    }
    let mut ids = Vec::new();
    if let Some(n) = app.session.active().and_then(|st| st.doc.node(NodeId(id))) {
        collect(n, &mut ids);
    }
    for id in ids {
        app.run("object.setProps",json!({"ids":[id],"data":{"sci-zoom-records":null,"sci-target":null,"sci-source":null,"sci-key":null,"sci-role":null,"sci-scale-options":null,"sci-pseudocolor":null}}))?;
    }
    Ok(())
}

fn cropped_fov(fov: &scale::Fov, frame: &[f64; 5], r: &Region) -> scale::Fov {
    let (sin, cos) = r.rotation.to_radians().sin_cos();
    let measure = |dx: f64, dy: f64| {
        if dx.abs() > 1e-8 && fov.width.is_none() || dy.abs() > 1e-8 && fov.height.is_none() {
            None
        } else {
            Some(((dx * fov.width.unwrap_or(0.) / frame[2]).powi(2) + (dy * fov.height.unwrap_or(0.) / frame[3]).powi(2)).sqrt())
        }
    };
    scale::Fov {
        width: measure(cos * r.width * frame[2], sin * r.width * frame[2]),
        height: measure(-sin * r.height * frame[3], cos * r.height * frame[3]),
    }
}

fn rebuild(
    app: &mut VectorcraftApp,
    b: &BoxInfo,
    region: &Region,
    s: &ZoomSettings,
    old: Option<&Record>,
    output: [f64; 5],
) -> Result<Record, String> {
    validate(region, s)?;
    let source = frame(app, b.id)?;
    let marker = marker_frame(&source, region)?;
    let marker_id = if let Some(old) = old {
        set_marker(app, old.marker, &marker)?;
        old.marker
    } else if region.key != 0 {
        let n = app.session.active().and_then(|st| st.doc.node(NodeId(region.key))).ok_or("手动 ROI 矩形已被移除")?;
        if region.key == b.id || n.kind_label() != "Rectangle" || !matches!(n.kind, NodeKind::Path { guide: false, clipping: false, .. }) {
            return Err("手动 ROI 必须为非裁剪矩形".into());
        }
        set_marker(app, region.key, &marker)?;
        region.key
    } else {
        let made =
            id(&app.run("shape.rectangle", json!({"x":marker[0]-marker[2]/2.,"y":marker[1]-marker[3]/2.,"width":marker[2],"height":marker[3]}))?)?;
        rotate(app, made, marker[4], marker[0], marker[1])?;
        made
    };
    let original_style = s.clone();
    let mut effective = s.clone();
    if !region.color.is_empty() {
        effective.color = region.color.clone();
    }
    let s = &effective;
    if s.dash == "original" && (old.is_some() || region.key != 0) {
        app.run("paint.setFill", json!({"ids":[marker_id],"none":true}))?;
    } else {
        stroke(app, marker_id, s)?;
    }

    app.run(
        "object.setProps",
        json!({"ids":[marker_id],"name":format!("ROI {}",region.name),"data":{"sci-role":"zoom-marker","sci-source":b.id.to_string()}}),
    )?;
    if output[2] <= 0. || output[3] <= 0. || output.iter().any(|n| !n.is_finite()) {
        return Err("输出边界无效".into());
    }
    let sx = output[2] / marker[2];
    let sy = output[3] / marker[3];
    let (sin, cos) = marker[4].to_radians().sin_cos();
    let matrix = [
        sx * cos,
        sy * sin,
        -sx * sin,
        sy * cos,
        output[0] - sx * cos * marker[0] + sx * sin * marker[1],
        output[1] - sy * sin * marker[0] - sy * cos * marker[1],
    ];
    let made = app.run("object.transform", json!({"ids":[b.id],"matrix":matrix,"copy":true,"strokes":false}))?;
    let copy = made.get("ids").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_u64).ok_or("缺少 ROI 源图副本")?;
    sanitize_copy(app, copy)?;
    let out = box_of(0, &output);
    let mask = id(&app.run("shape.rectangle", json!({"x":out.x,"y":out.y,"width":out.w,"height":out.h}))?)?;
    app.run("select.set", json!({"ids":[copy,mask]}))?;
    let clip = id(&app.run("object.clippingMask.make", json!({}))?)?;
    let border = id(&app.run("shape.rectangle", json!({"x":out.x,"y":out.y,"width":out.w,"height":out.h}))?)?;
    stroke(app, border, s)?;
    if s.dash == "original" {
        copy_stroke(app, marker_id, border)?;
    }
    app.run("select.set", json!({"ids":[clip,border]}))?;
    let temporary = id(&app.run("object.group", json!({}))?)?;
    let output_id = if let Some(old) = old {
        clear(app, &children(app, old.output))?;
        let new = children(app, temporary);
        app.run("layer.move", json!({"ids":new,"target":old.output,"place":"inside"}))?;
        clear(app, &[temporary])?;
        app.run("select.set", json!({"ids":[old.output]}))?;
        app.run("object.resetBoundingBox", json!({}))?;
        old.output
    } else {
        temporary
    };
    app.run("object.setProps",json!({"ids":[output_id],"name":format!("放大图 {}",region.name),"data":{"sci-role":"zoom-output","sci-source":b.id.to_string(),"sci-key":marker_id.to_string()}}))?;
    if let Some(fov) = scale::stored(app, b.id)? {
        let cropped = cropped_fov(&fov, &source, region);
        app.run("object.setProps", json!({"ids":[output_id],"data":{"sci-fov":serde_json::to_string(&cropped).map_err(|e|e.to_string())?}}))?;
        if s.inherit_scale {
            let detail = app.run("document.node", json!({"id":b.id}))?;
            if let Some(raw) = attribute(&detail, "sci-scale-options") {
                let mut options: scale::ScaleSettings = serde_json::from_str(raw).map_err(|_| "Invalid inherited scale settings")?;
                options.auto_group = false;
                if !region.scale_unit.is_empty() {
                    options.length *= scale::factor(&options.unit)? / scale::factor(&region.scale_unit)?;
                    options.unit = region.scale_unit.clone();
                }
                if !region.scale_length.trim().is_empty() {
                    options.length = region.scale_length.trim().parse::<f64>().map_err(|_| "Invalid ROI scale length")?;
                }
                if options.length > 0. {
                    let group = scale::draw(app, &BoxInfo { id: output_id, ..out.clone() }, &cropped, &options)?;
                    app.run("layer.move", json!({"ids":[group],"target":output_id,"place":"inside"}))?;
                }
                app.run(
                    "object.setProps",
                    json!({"ids":[output_id],"data":{"sci-scale-options":serde_json::to_string(&options).map_err(|e|e.to_string())?}}),
                )?;
            }
        }
    }
    rotate(app, output_id, output[4], output[0], output[1])?;
    if let Some(old) = old {
        clear(app, &old.guides)?;
    }
    let guides = guidelines(app, b, marker_id, output_id, s)?;
    let mut r = region.clone();
    r.key = marker_id;
    let mut style = original_style;
    style.regions.clear();
    Ok(Record {
        region: r,
        source: b.id,
        marker: marker_id,
        output: output_id,
        guides,
        source_frame: source,
        marker_frame: frame(app, marker_id)?,
        output_frame: frame(app, output_id)?,
        signature: signature(app, b.id)?,
        style,
    })
}

fn copy_stroke(app: &mut VectorcraftApp, source: u64, target: u64) -> Result<(), String> {
    let n = app.run("document.node", json!({"id":source,"summary":true}))?;
    if let Some(color) = n.get("stroke").and_then(Value::as_str).filter(|s| s.starts_with('#')) {
        app.run("paint.setStroke", json!({"ids":[target],"color":color}))?;
    }
    let mut options = n.get("strokeOptions").and_then(Value::as_object).cloned().unwrap_or_default();
    options.insert("ids".into(), json!([target]));
    if let Some(weight) = n.get("strokeWidth") {
        options.insert("weight".into(), weight.clone());
    }
    app.run("stroke.set", Value::Object(options))?;
    Ok(())
}

fn guidelines(app: &mut VectorcraftApp, b: &BoxInfo, marker: u64, output: u64, s: &ZoomSettings) -> Result<Vec<u64>, String> {
    if !s.guides {
        return Ok(Vec::new());
    }
    let st = app.session.active().ok_or("没有打开文档")?;
    let m = st.doc.oriented_bounds(&[NodeId(marker)], false).ok_or("缺少 ROI")?.corners();
    let o = st.doc.oriented_bounds(&[NodeId(output)], false).ok_or("缺少输出对象")?.corners();
    let pairs = match s.placement.as_str() {
        "left" => [(0, 1), (3, 2)],
        "top" => [(0, 3), (1, 2)],
        "bottom" => [(3, 0), (2, 1)],
        _ => [(1, 0), (2, 3)],
    };
    let mut ids = Vec::new();
    for (a, z) in pairs {
        let start = m.get(a).ok_or("缺少 ROI 角点")?;
        let end = o.get(z).ok_or("缺少输出角点")?;
        let (mut x, mut y) = (end.x, end.y);
        if s.extent == "inside" {
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let mut t = 1.0_f64;
            for n in [
                if dx > 1e-9 {
                    (b.x + b.w - start.x) / dx
                } else if dx < -1e-9 {
                    (b.x - start.x) / dx
                } else {
                    1.
                },
                if dy > 1e-9 {
                    (b.y + b.h - start.y) / dy
                } else if dy < -1e-9 {
                    (b.y - start.y) / dy
                } else {
                    1.
                },
            ] {
                if n >= 0. {
                    t = t.min(n);
                }
            }
            x = start.x + t * dx;
            y = start.y + t * dy;
        }
        let id = id(&app.run("path.create", json!({"d":format!("M {} {} L {} {}",start.x,start.y,x,y)}))?)?;
        stroke(app, id, s)?;
        if s.dash == "original" {
            copy_stroke(app, marker, id)?;
        }
        app.run("object.setProps", json!({"ids":[id],"name":"ROI 引导线","data":{"sci-role":"zoom-guide","sci-source":b.id.to_string()}}))?;
        ids.push(id);
    }
    Ok(ids)
}

pub(super) fn repoint_source(app: &mut VectorcraftApp, old: u64, new: u64) -> Result<(), String> {
    let mut records = records(app, old)?;
    for r in &mut records {
        r.source = new;
    }
    let old_key = old.to_string();
    let new_key = new.to_string();
    fn visit(n: &Node, old: &str, out: &mut Vec<(u64, serde_json::Map<String, Value>)>) {
        if let Some(attrs) = n.extras().attrs.as_ref() {
            let mut patch = serde_json::Map::new();
            for (key, value) in &attrs.data {
                if matches!(key.as_str(), "sci-target" | "sci-source") && value == old {
                    patch.insert(key.clone(), Value::Null);
                }
            }
            if !patch.is_empty() {
                out.push((n.id.0, patch));
            }
        }
        if let Some(children) = n.children() {
            for child in children {
                visit(child, old, out);
            }
        }
    }
    let mut nodes = Vec::new();
    if let Some(st) = app.session.active() {
        for n in &st.doc.layers {
            visit(n, &old_key, &mut nodes);
        }
    }
    for (id, mut data) in nodes {
        for value in data.values_mut() {
            *value = json!(new_key);
        }
        app.run("object.setProps", json!({"ids":[id],"data":data}))?;
    }
    if !records.is_empty() {
        save(app, new, &records)?;
        app.run("object.setProps", json!({"ids":[old],"data":{"sci-zoom-records":null}}))?;
    }
    Ok(())
}

pub(super) fn apply(app: &mut VectorcraftApp, s: &ZoomSettings) -> Result<(), String> {
    if s.regions.len() > 32 {
        return Err("请使用1至32个 ROI 区域".into());
    }
    for r in &s.regions {
        validate(r, s)?;
    }
    let (sources, _) = sources(app)?;
    transaction(app, "创建或更新局部放大图", sources.iter().map(|b| b.id).collect(), |app| {
        for (source_index, b) in sources.iter().enumerate() {
            let old = records(app, b.id)?;
            let source = frame(app, b.id)?;
            let mut out = if source_index == 0 { Vec::new() } else { old.clone() };
            for (i, r) in s.regions.iter().enumerate() {
                if source_index > 0 && r.key != 0 {
                    continue;
                }
                let existing =
                    old.iter().find(|old| old.region.key == r.key && r.key != 0 && frame(app, old.marker).is_ok() && frame(app, old.output).is_ok());
                let output = if let Some(old) = existing { frame(app, old.output)? } else { initial_output(b, &source, r, s, i) };
                out.push(rebuild(app, b, r, s, existing, output)?);
            }
            if source_index == 0 {
                for old in &old {
                    if !out.iter().any(|r| r.region.key == old.region.key) {
                        clear(app, &[old.marker, old.output])?;
                        clear(app, &old.guides)?;
                    }
                }
            }
            save(app, b.id, &out)?;
        }
        Ok(())
    })
}

pub(super) fn load(app: &mut VectorcraftApp) -> Result<ZoomSettings, String> {
    let (sources, _) = sources(app)?;
    let first = sources.first().ok_or("请选择源对象")?;
    let records = records(app, first.id)?;
    let mut s = records.first().map(|r| r.style.clone()).unwrap_or_else(|| app.ui.scientific.zoom.clone());
    if records.is_empty() {
        for r in &mut s.regions {
            r.key = 0;
        }
    } else {
        s.regions = records.iter().map(|r| r.region.clone()).collect();
    }
    Ok(s)
}

pub(super) fn pick(app: &mut VectorcraftApp, s: &mut ZoomSettings) -> Result<(), String> {
    let (all, nodes) = selection(app)?;
    let image = all.iter().find(|b| nodes.get(&b.id).is_some_and(has_image)).ok_or("请选择图片和矩形")?;
    let marker =
        all.iter().find(|b| b.id != image.id && nodes.get(&b.id).is_some_and(|n| n.get("anchors").is_some())).ok_or("请同时选择图片和矩形")?;
    if s.regions.len() >= 32 {
        return Err("最多设置32个区域".into());
    }
    let r =
        normalized(&frame(app, image.id)?, &frame(app, marker.id)?, &Region { name: format!("ROI {}", s.regions.len() + 1), ..Default::default() });
    validate(&r, s)?;
    marker_frame(&frame(app, image.id)?, &r)?;
    let mut r = r;
    r.key = marker.id;
    if s.use_rectangle_color {
        r.color = nodes.get(&marker.id).and_then(|n| n.get("stroke")).and_then(Value::as_str).filter(|c| c.starts_with('#')).unwrap_or("").into();
    }
    if s.regions.len() == 1
        && s.regions.first().is_some_and(|r| {
            let mut initial = r.clone();
            initial.color.clear();
            initial == Region::default()
        })
    {
        r.name = "ROI 1".into();
        s.regions.clear();
    }
    s.regions.push(r);
    Ok(())
}

pub(super) fn sync(app: &mut VectorcraftApp, automatic: bool) -> Result<(), String> {
    fn scan(n: &Node, out: &mut Vec<(u64, Vec<Record>)>) -> Result<(), String> {
        if let Some(raw) = raw_records(n) {
            let records: Vec<Record> = serde_json::from_str(raw).map_err(|_| "Invalid ROI records")?;
            if records.len() > 32 {
                return Err("存储的 ROI 记录过多".into());
            }
            out.push((n.id.0, records));
        }
        if let Some(children) = n.children() {
            for n in children {
                scan(n, out)?;
            }
        }
        Ok(())
    }
    let st = app.session.active().ok_or("没有打开文档")?;
    let mut all = Vec::new();
    for n in &st.doc.layers {
        scan(n, &mut all)?;
    }
    if all.is_empty() {
        return Ok(());
    }
    let selected: Vec<_> = st.selection.objects.iter().map(|id| id.0).collect();
    let before = st.history.undo.len();
    let merge = automatic
        && app.ui.scientific_zoom_tracker.as_ref().is_some_and(|t| {
            t.index == app.session.active_index()
                && t.title == st.title()
                && st.history.undo.last().is_some_and(|e| std::sync::Arc::ptr_eq(&e.doc, &t.document))
        });
    let mut tasks = Vec::new();
    for (source, records) in &all {
        for r in records {
            check_record(app, *source, r)?;
        }
        let sf = frame(app, *source)?;
        let signature = signature(app, *source)?;
        for r in records {
            let mf = frame(app, r.marker);
            let of = frame(app, r.output);
            if mf.is_err() || of.is_err() {
                tasks.push((*source, r.clone(), None));
                continue;
            }
            let mf = mf?;
            let of = of?;
            if changed(&sf, &r.source_frame) || changed(&mf, &r.marker_frame) || changed(&of, &r.output_frame) || signature != r.signature {
                let mut r = r.clone();
                if !changed(&sf, &r.source_frame) && changed(&mf, &r.marker_frame) {
                    r.region = normalized(&sf, &mf, &r.region);
                }
                tasks.push((*source, r, Some(of)));
            }
        }
    }
    if tasks.is_empty() {
        return Ok(());
    }
    transaction(app, "同步放大图关联", selected, |app| {
        for (source, old, output) in &tasks {
            let records = all.iter_mut().find(|(id, _)| id == source).ok_or("源对象已被移除")?;
            if let Some(output) = output {
                let b = BoxInfo { id: *source, ..bounds(&app.run("document.node", json!({"id":source,"summary":true}))?)? };
                let next = rebuild(app, &b, &old.region, &old.style, Some(old), *output)?;
                if let Some(record) = records.1.iter_mut().find(|r| r.region.key == old.region.key) {
                    *record = next;
                }
            } else {
                // Deleting an association endpoint unlinks the remaining crop, preserving its pixels.
                clear(app, &old.guides)?;
                records.1.retain(|r| r.region.key != old.region.key);
                if app.session.active().is_some_and(|st| st.doc.node(NodeId(old.output)).is_some()) {
                    app.run("object.setProps", json!({"ids":[old.output],"data":{"sci-source":null,"sci-key":null,"sci-role":null}}))?;
                }
            }
        }
        for (source, records) in &all {
            save(app, *source, records)?;
        }
        Ok(())
    })?;
    if merge
        && let Some(st) = app.session.active_mut()
        && st.history.undo.len() == before + 1
    {
        st.history.undo.pop();
    }
    Ok(())
}

pub(crate) fn tick(app: &mut VectorcraftApp) {
    if !app.ui.scientific.zoom.auto_update || app.session.in_interaction() || app.ui.dialog.is_some() {
        return;
    }
    let Some(st) = app.session.active() else { return };
    if st.undo_group.is_some() {
        return;
    }
    if app
        .ui
        .scientific_zoom_tracker
        .as_ref()
        .is_some_and(|t| t.index == app.session.active_index() && t.title == st.title() && t.revision == st.revision)
    {
        return;
    }
    if let Err(e) = sync(app, true) {
        app.status(e);
    }
    app.ui.scientific_zoom_tracker = app.session.active().map(|st| Tracker {
        index: app.session.active_index(),
        title: st.title(),
        revision: st.revision,
        document: st.doc.clone(),
    });
}

pub(super) fn show(app: &mut VectorcraftApp, ui: &mut Ui, s: &mut Settings) {
    egui::CollapsingHeader::new("局部放大与关联更新").show(ui, |ui| {
        ui.small("相对坐标为 0–1；也可同时选择原图和 ROI 矩形读取区域。");
        let default_color = s.zoom.color.clone();
        let mut remove = None;
        for (i, r) in s.zoom.regions.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(100.));
                    if ui.small_button("删除区域").clicked() {
                        remove = Some(i);
                    }
                });
                egui::Grid::new(("roi", i)).num_columns(2).show(ui, |ui| {
                    for (name, value) in [("X", &mut r.x), ("Y", &mut r.y), ("宽", &mut r.width), ("高", &mut r.height)] {
                        ui.label(name);
                        ui.add(egui::DragValue::new(value).speed(0.005).range(0.0..=1.0));
                        ui.end_row();
                    }
                    ui.label("旋转角度");
                    ui.add(egui::DragValue::new(&mut r.rotation).range(-180.0..=180.0));
                    ui.end_row();
                });
                if r.color.is_empty() {
                    r.color = default_color.clone();
                }
                super::colors::field(app, ui, &mut r.color, &format!("zoom-region-{i}"));
                ui.add(egui::TextEdit::singleline(&mut r.scale_length).hint_text("本区域比例尺长度（空白继承，0隐藏）"));
                egui::ComboBox::from_id_salt(("roi-scale-unit", i))
                    .selected_text(if r.scale_unit.is_empty() { "继承比例尺单位" } else { &r.scale_unit })
                    .show_ui(ui, |ui| {
                        for v in ["", "nm", "um", "mm", "cm", "m", "inch"] {
                            ui.selectable_value(&mut r.scale_unit, v.into(), if v.is_empty() { "继承" } else { v });
                        }
                    });
            });
        }
        if let Some(i) = remove {
            s.zoom.regions.remove(i);
        }
        if s.zoom.regions.len() < 32 && ui.button("新增 ROI 区域").clicked() {
            s.zoom.regions.push(Region { name: format!("ROI {}", s.zoom.regions.len() + 1), ..Default::default() });
        }
        egui::ComboBox::from_id_salt("sci-zoom-placement").selected_text(&s.zoom.placement).show_ui(ui, |ui| {
            for (v, n) in [("left", "左"), ("right", "右"), ("top", "上"), ("bottom", "下")] {
                ui.selectable_value(&mut s.zoom.placement, v.into(), n);
            }
        });
        ui.horizontal(|ui| {
            ui.label("倍率（0自动齐边）");
            ui.add(egui::DragValue::new(&mut s.zoom.magnification).range(0.0..=100.));
        });
        ui.horizontal(|ui| {
            ui.label("间隔 pt");
            ui.add(egui::DragValue::new(&mut s.zoom.gap).range(0.0..=10000.));
        });
        ui.label("新区域的默认颜色");
        super::colors::field(app, ui, &mut s.zoom.color, "zoom");
        if ui.button("将默认颜色应用到全部区域").clicked() {
            for r in &mut s.zoom.regions {
                r.color = s.zoom.color.clone();
            }
        }
        ui.horizontal(|ui| {
            ui.label("线宽 pt");
            ui.add(egui::DragValue::new(&mut s.zoom.stroke).range(0.01..=100.));
        });
        egui::ComboBox::from_id_salt("sci-zoom-dash").selected_text(&s.zoom.dash).show_ui(ui, |ui| {
            for v in ["solid", "dash", "dot", "dashdot", "dashdotdot", "original"] {
                ui.selectable_value(&mut s.zoom.dash, v.into(), v);
            }
        });
        ui.checkbox(&mut s.zoom.use_rectangle_color, "读取矩形时沿用其描边颜色");
        ui.checkbox(&mut s.zoom.guides, "显示引导线");
        ui.checkbox(&mut s.zoom.inherit_scale, "继承原图比例尺");
        ui.checkbox(&mut s.zoom.auto_update, "自动同步区域、原图与放大图变化");
        egui::ComboBox::from_id_salt("sci-guide-extent").selected_text(&s.zoom.extent).show_ui(ui, |ui| {
            ui.selectable_value(&mut s.zoom.extent, "inside".into(), "只画到原图边缘");
            ui.selectable_value(&mut s.zoom.extent, "across".into(), "连接至放大图");
        });
        for (op, title) in
            [("zoom_load", "读取已有区域"), ("zoom_pick", "读取所选矩形为新区域"), ("zoom_apply", "创建 / 更新放大图"), ("zoom_sync", "立即同步关联")]
        {
            if ui.button(title).clicked() {
                match app.run("ui.scientific", json!({"operation":op,"settings":s})) {
                    Ok(_) => {
                        *s = app.ui.scientific.clone();
                        app.status("放大图操作完成");
                    }
                    Err(e) => app.status(e),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_region_round_trips_in_source_coordinates() {
        let source = [100., 80., 200., 100., 25.];
        let r = Region { x: 0.3, y: 0.3, width: 0.2, height: 0.2, rotation: 15., ..Default::default() };
        let marker = marker_frame(&source, &r).unwrap();
        let read = normalized(&source, &marker, &r);
        assert!((read.x - r.x).abs() < 1e-8);
        assert!((read.rotation - r.rotation).abs() < 1e-8);
        assert!(marker_frame(&source, &Region { x: 0., y: 0., width: 0.5, height: 0.5, rotation: 45., ..Default::default() }).is_err());
    }
    fn fixture() -> (VectorcraftApp, u64) {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({})).unwrap();
        let image = image::RgbaImage::from_pixel(64, 48, image::Rgba([80, 140, 160, 255]));
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut output, image::ImageFormat::Png).unwrap();
        let placed = app
            .run(
                "file.place",
                json!({"name":"synthetic.png","dataBase64":vectorcraft_format::base64_encode(&output.into_inner()),"at":[30,40],"link":false}),
            )
            .unwrap();
        let source = placed["ids"][0].as_u64().unwrap();
        app.run("object.transform", json!({"ids":[source],"matrix":[3.125,0,0,3.125,0,0]})).unwrap();
        app.run("select.set", json!({"ids":[source]})).unwrap();
        (app, source)
    }
    #[test]
    fn editable_roi_updates_keep_output_identity_and_undo() {
        let (mut app, source) = fixture();
        app.run("ui.scientific", json!({"operation":"zoom_apply","settings":{"zoom":{"inherit_scale":false}}})).unwrap();
        let first = records(&app, source).unwrap();
        assert_eq!(first.len(), 1);
        let original = app.run("document.inspect", json!({})).unwrap();
        let mut settings = app.ui.scientific.zoom.clone();
        settings.regions[0].x = 0.3;
        app.run("ui.scientific", json!({"operation":"zoom_apply","settings":{"zoom":settings}})).unwrap();
        let updated = records(&app, source).unwrap();
        assert_eq!(updated[0].output, first[0].output);
        assert_eq!(updated[0].marker, first[0].marker);
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(app.run("document.inspect", json!({})).unwrap()["layers"], original["layers"]);
        let before = app.run("document.inspect", json!({})).unwrap();
        assert!(app.run("ui.scientific", json!({"operation":"zoom_apply","settings":{"zoom":{"regions":[{"x":0.95,"width":0.5}]}}})).is_err());
        assert_eq!(app.run("document.inspect", json!({})).unwrap(), before);
    }
    #[test]
    fn moved_roi_syncs_crop_and_preserves_output_size() {
        let (mut app, source) = fixture();
        app.run("ui.scientific", json!({"operation":"zoom_apply"})).unwrap();
        let old = records(&app, source).unwrap().remove(0);
        app.run("object.transform", json!({"ids":[old.marker],"matrix":[1,0,0,1,10,0]})).unwrap();
        sync(&mut app, false).unwrap();
        let new = records(&app, source).unwrap().remove(0);
        assert!(new.region.x > old.region.x);
        assert_eq!(new.output, old.output);
        assert_eq!(new.output_frame, old.output_frame);
    }
    #[test]
    fn automatic_sync_merges_with_source_edit_and_undo_redo_restores_both() {
        let (mut app, source) = fixture();
        app.run("ui.scientific", json!({"operation":"zoom_apply"})).unwrap();
        tick(&mut app);
        let before = app.run("document.inspect", json!({})).unwrap();
        let undo = app.session.active().unwrap().history.undo.len();
        let marker = records(&app, source).unwrap()[0].marker;
        app.run("object.transform", json!({"ids":[source],"matrix":[1,0,0,1,12,4]})).unwrap();
        tick(&mut app);
        assert_eq!(app.session.active().unwrap().history.undo.len(), undo + 1);
        let after = app.run("document.inspect", json!({})).unwrap();
        assert!(frame(&app, marker).unwrap()[0] > records(&app, source).unwrap()[0].source_frame[0] - 100.);
        app.run("edit.undo", json!({})).unwrap();
        tick(&mut app);
        assert_eq!(app.run("document.inspect", json!({})).unwrap()["layers"], before["layers"]);
        app.run("edit.redo", json!({})).unwrap();
        tick(&mut app);
        assert_eq!(app.run("document.inspect", json!({})).unwrap()["layers"], after["layers"]);
    }
}
