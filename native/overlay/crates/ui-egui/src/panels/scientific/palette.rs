//! Editable scientific palettes. The library is application state, never document paint.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Palette {
    pub group: String,
    pub name: String,
    pub colors: Vec<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub modified: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub groups: Vec<String>,
    pub palettes: Vec<Palette>,
    pub active: String,
    pub name: String,
    pub color_text: String,
    pub group_name: String,
    pub old_group: String,
    pub old_name: String,
    pub target: String,
    pub copy_rgb: bool,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            groups: ["期刊配色", "分类配色", "连续配色", "发散配色"].map(str::to_string).to_vec(),
            palettes: serde_json::from_str(include_str!("palettes.json")).unwrap_or_default(),
            active: "分类配色".into(),
            name: String::new(),
            color_text: String::new(),
            group_name: String::new(),
            old_group: String::new(),
            old_name: String::new(),
            target: "labels".into(),
            copy_rgb: false,
        }
    }
}

fn name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 {
        return Err("名称必须为1至120字节".into());
    }
    Ok(value.into())
}

pub fn command(app: &mut VectorcraftApp, op: &str, p: &Value) -> Result<Value, String> {
    if op == "palette_apply" {
        return apply_fill(app, p);
    }
    let mut lib = app.ui.scientific_library.clone();
    let text = |key| p.get(key).and_then(Value::as_str).unwrap_or("");
    match op {
        "palette_inspect" => return serde_json::to_value(&lib).map_err(|e| e.to_string()),
        "palette_group_save" => {
            let next = name(text("name"))?;
            let old = text("oldName");
            if old != next && lib.groups.contains(&next) {
                return Err("分组已存在".into());
            }
            if !old.is_empty() {
                let group = lib.groups.iter_mut().find(|g| g.as_str() == old).ok_or("找不到分组")?;
                *group = next.clone();
                for palette in &mut lib.palettes {
                    if palette.group == old {
                        palette.group = next.clone();
                    }
                }
            } else {
                if lib.groups.len() >= 32 {
                    return Err("最多保存32个分组".into());
                }
                lib.groups.push(next.clone());
            }
            lib.active = next;
        }
        "palette_group_delete" => {
            let group = name(text("name"))?;
            if !lib.groups.contains(&group) {
                return Err("找不到分组".into());
            }
            lib.groups.retain(|g| g != &group);
            lib.palettes.retain(|p| p.group != group);
            lib.active = lib.groups.first().cloned().unwrap_or_default();
        }
        "palette_save" => {
            let group = name(text("group"))?;
            let next = name(text("name"))?;
            if !lib.groups.contains(&group) {
                return Err("找不到分组".into());
            }
            let colors = p.get("colors").and_then(Value::as_array).filter(|a| !a.is_empty() && a.len() <= 64).ok_or("请使用1至64个颜色")?;
            let colors = colors
                .iter()
                .map(|v| {
                    crate::panels::color::parse_hex(v.as_str().unwrap_or("")).map(|c| c.to_hex().to_uppercase()).ok_or("配色颜色无效".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let old_name = text("oldName");
            let old_group = text("oldGroup");
            let old = lib.palettes.iter().find(|p| p.name == old_name && p.group == old_group).cloned();
            if lib.palettes.iter().any(|p| p.name == next && p.group == group && !(p.name == old_name && p.group == old_group)) {
                return Err("配色已存在".into());
            }
            if old.is_none() && lib.palettes.len() >= 256 {
                return Err("最多保存256套配色".into());
            }
            lib.palettes.retain(|p| !(p.name == old_name && p.group == old_group));
            lib.palettes.push(Palette { group, name: next, colors, source: old.map(|p| p.source).unwrap_or_default(), modified: true });
            lib.name.clear();
            lib.color_text.clear();
            lib.old_name.clear();
            lib.old_group.clear();
        }
        "palette_delete" => {
            let before = lib.palettes.len();
            lib.palettes.retain(|p| !(p.name == text("name") && p.group == text("group")));
            if before == lib.palettes.len() {
                return Err("找不到配色".into());
            }
        }
        _ => return Err("未知的配色操作".into()),
    }
    app.ui.scientific_library = lib;
    Ok(json!({"ok":true}))
}

fn apply_fill(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let color = crate::panels::color::parse_hex(p.get("color").and_then(Value::as_str).unwrap_or("")).ok_or("填充颜色无效")?.to_hex();
    let (selected, nodes) = selection(app)?;
    fn paths(node: &Value, ids: &mut Vec<u64>) {
        if node.get("locked").and_then(Value::as_bool).unwrap_or(false)
            || !node.get("visible").and_then(Value::as_bool).unwrap_or(true)
            || node.get("template").and_then(Value::as_bool).unwrap_or(false)
        {
            return;
        }
        let kind = node.get("kind").and_then(Value::as_str).unwrap_or("");
        if node.get("anchors").is_some() && kind != "Guide" {
            if let Some(id) = node.get("id").and_then(Value::as_u64) {
                ids.push(id);
            }
        } else if matches!(kind, "Group" | "Clip Group" | "Compound Path") {
            for child in node.get("children").and_then(Value::as_array).into_iter().flatten().skip(usize::from(kind == "Clip Group")) {
                paths(child, ids);
            }
        }
    }
    let mut ids = Vec::new();
    for b in &selected {
        if let Some(n) = nodes.get(&b.id) {
            paths(n, &mut ids);
        }
    }
    if let Some(st) = app.session.active() {
        ids.retain(|id| {
            let Some(node) = st.doc.node(vectorcraft_doc::NodeId(*id)) else { return false };
            if !matches!(node.kind, vectorcraft_doc::NodeKind::Path { guide: false, clipping: false, .. }) {
                return false;
            }
            let Some(path) = st.doc.index_path(vectorcraft_doc::NodeId(*id)) else { return false };
            for end in 1..path.len() {
                if path.get(end) == Some(&0)
                    && st.doc.node_at(&path[..end]).is_some_and(|n| {
                        matches!(n.kind, vectorcraft_doc::NodeKind::Group { clip: true, .. } | vectorcraft_doc::NodeKind::Layer { clip: true, .. })
                    })
                {
                    return false;
                }
            }
            true
        });
    }
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() || ids.len() > 200 {
        return Err("请选择1至200个可填充路径；跳过图片、文字、参考线与裁剪路径".into());
    }
    transaction(app, "科研色卡填充", selected.iter().map(|b| b.id).collect(), |app| {
        app.run("paint.setFill", json!({"ids":ids,"color":color}))?;
        Ok(())
    })?;
    Ok(json!({"filled":ids.len()}))
}

fn act(app: &mut VectorcraftApp, lib: &mut Library, op: &str, mut p: Value) {
    // Persist draft fields first, so successful commands do not discard UI edits.
    app.ui.scientific_library = lib.clone();
    p["operation"] = json!(op);
    match app.run("ui.scientific", p) {
        Ok(_) => {
            *lib = app.ui.scientific_library.clone();
            app.status("色卡已保存；未修改画稿");
        }
        Err(e) => app.status(e),
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    egui::CollapsingHeader::new("科研色卡").show(ui, |ui| {
        let mut lib = app.ui.scientific_library.clone();
        ui.small("左键选为参数颜色；右键复制颜色值。");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut lib.target, "labels".into(), "文字");
            ui.selectable_value(&mut lib.target, "borders".into(), "边框");
            ui.checkbox(&mut lib.copy_rgb, "复制 RGB");
        });
        egui::ComboBox::from_id_salt("sci-palette-group").selected_text(&lib.active).show_ui(ui, |ui| {
            for group in &lib.groups { ui.selectable_value(&mut lib.active, group.clone(), group); }
        });
        if lib.active=="期刊配色"{ui.small("此组为 SCI 原创方案，不是期刊指定色表。");}
        let mut chosen = None;
        let mut edit = None;
        let mut delete = None;
        for palette in lib.palettes.iter().filter(|p| p.group == lib.active) {
            ui.horizontal(|ui| {
                ui.label(&palette.name);
                if ui.small_button("编辑").clicked() { edit = Some(palette.clone()); }
                if ui.small_button("删除").clicked() { delete = Some(palette.name.clone()); }
            });
            ui.horizontal_wrapped(|ui| {
                for color in &palette.colors {
                    if let Some(parsed) = crate::panels::color::parse_hex(color) {
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                        ui.painter().rect_filled(rect, 3.0, crate::panels::c32(&parsed));
                        let response = response.on_hover_text(color);
                        if response.clicked() { chosen = Some(color.clone()); }
                        if response.secondary_clicked() {
                            let [r,g,b,_] = parsed.to_rgba8(1.0);
                            ui.ctx().copy_text(if lib.copy_rgb {format!("rgb({r}, {g}, {b})")} else {color.clone()});
                            app.status("颜色已复制");
                        }
                    }
                }
            });
        }
        if let Some(color) = chosen
            && let Err(e) = app.run("ui.scientific", json!({"operation":"set_color","target":lib.target,"color":color})) { app.status(e); }
        if let Some(p) = edit { lib.name = p.name.clone(); lib.color_text = p.colors.join(" "); lib.old_name = p.name; lib.old_group = p.group; }
        if let Some(name) = delete { let group = lib.active.clone(); act(app,&mut lib,"palette_delete",json!({"name":name,"group":group})); }
        let fill_color = if lib.target == "borders" { app.ui.scientific.border_color.clone() } else { app.ui.scientific.label_color.clone() };
        if ui.button("将当前参数色填充到所选形状").clicked()
            && let Err(e) = app.run("ui.scientific", json!({"operation":"palette_apply","color":fill_color})) { app.status(e); }
        ui.separator();
        ui.add(egui::TextEdit::singleline(&mut lib.name).hint_text("色卡名称"));
        ui.add(egui::TextEdit::multiline(&mut lib.color_text).desired_rows(2).hint_text("#RGB 或 #RRGGBB，以空格分隔"));
        if ui.button("保存色卡到当前分组").clicked() {
            let p = json!({"name":lib.name,"group":lib.active,"oldName":lib.old_name,"oldGroup":lib.old_group,"colors":lib.color_text.split_whitespace().collect::<Vec<_>>()});
            act(app,&mut lib,"palette_save",p);
        }
        egui::CollapsingHeader::new("管理分组").show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut lib.group_name).hint_text("分组名称"));
            if ui.button("新增分组").clicked() { let name=lib.group_name.clone(); act(app,&mut lib,"palette_group_save",json!({"name":name})); }
            if ui.button("重命名当前分组").clicked() { let p=json!({"name":lib.group_name,"oldName":lib.active}); act(app,&mut lib,"palette_group_save",p); }
            if ui.button("删除当前分组及其色卡").clicked() { let name=lib.active.clone(); act(app,&mut lib,"palette_group_delete",json!({"name":name})); }
        });
        app.ui.scientific_library = lib;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_edits_are_atomic_and_do_not_touch_the_document() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({})).unwrap();
        let before = app.run("document.inspect", json!({})).unwrap();
        assert_eq!(app.ui.scientific_library.palettes.len(), 23);
        command(&mut app, "palette_group_save", &json!({"name":"测试"})).unwrap();
        command(&mut app, "palette_save", &json!({"group":"测试","name":"双色","colors":["#abc","123456"]})).unwrap();
        let saved = serde_json::to_value(&app.ui.scientific_library).unwrap();
        assert!(command(&mut app, "palette_save", &json!({"group":"测试","name":"双色","colors":["bad-color"]})).is_err());
        assert_eq!(serde_json::to_value(&app.ui.scientific_library).unwrap(), saved);
        command(&mut app, "palette_group_save", &json!({"oldName":"测试","name":"重命名"})).unwrap();
        assert!(app.ui.scientific_library.palettes.iter().any(|p| p.group == "重命名" && p.colors == vec!["#AABBCC", "#123456"]));
        command(&mut app, "palette_group_delete", &json!({"name":"重命名"})).unwrap();
        assert_eq!(app.run("document.inspect", json!({})).unwrap(), before);
    }
}
