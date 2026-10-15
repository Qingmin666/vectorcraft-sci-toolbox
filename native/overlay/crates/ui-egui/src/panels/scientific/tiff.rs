//! Bounded, header-only TIFF calibration reader. No pixel decode or printing-DPI inference.
use super::scale::{Fov, factor};
use crate::VectorcraftApp;
use std::io::{Read, Seek, SeekFrom};

fn integer(data: &[u8], little: bool) -> u64 {
    data.iter().enumerate().fold(0, |v, (i, b)| v | ((*b as u64) << if little { 8 * i } else { 8 * (data.len() - 1 - i) }))
}
fn bytes(file: &mut std::fs::File, at: u64, size: usize) -> Result<Vec<u8>, String> {
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if size > 1_048_576 || at.checked_add(size as u64).is_none_or(|n| n > len) {
        return Err("TIFF 元数据范围无效".into());
    }
    file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    let mut data = vec![0; size];
    file.read_exact(&mut data).map_err(|e| e.to_string())?;
    Ok(data)
}
fn attr(text: &str, key: &str) -> Option<String> {
    for (at, _) in text.match_indices(key) {
        let rest = text.get(at + key.len()..)?.trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '\'' && quote != '"' {
            continue;
        }
        return Some(rest.get(1..)?.split(quote).next()?.replace("&#181;", "µ").replace("&#956;", "μ"));
    }
    None
}

fn unit(unit: &str) -> Result<f64, String> {
    match unit.trim().to_lowercase().as_str() {
        "micron" | "microns" | "micrometer" | "micrometers" => Ok(1.),
        other => factor(other),
    }
}
fn metadata(desc: &str, width: f64, height: f64, xres: Option<f64>, yres: Option<f64>) -> Result<Option<Fov>, String> {
    if let Some(start) = desc.find("Pixels") {
        let pixels = desc.get(start..).and_then(|s| s.split('>').next()).unwrap_or("");
        let physical = |axis: &str, pixels_count: f64| -> Result<Option<f64>, String> {
            let Some(size) = attr(pixels, &format!("PhysicalSize{axis}")) else { return Ok(None) };
            let size = size.parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.).ok_or("OME 物理像素尺寸无效")?;
            let unit = attr(pixels, &format!("PhysicalSize{axis}Unit")).unwrap_or_else(|| "um".into());
            Ok(Some(pixels_count * size * super::tiff::unit(&unit)?))
        };
        let fov = Fov { width: physical("X", width)?, height: physical("Y", height)? };
        if fov.width.is_some() || fov.height.is_some() {
            return Ok(Some(fov));
        }
    }
    if desc.lines().any(|line| line.trim_start().starts_with("ImageJ="))
        && let Some(unit_name) = desc.lines().find_map(|line| line.trim().strip_prefix("unit="))
    {
        let factor = unit(unit_name)?;
        let fov = Fov {
            width: xres.filter(|r| r.is_finite() && *r > 0.).map(|r| width / r * factor),
            height: yres.filter(|r| r.is_finite() && *r > 0.).map(|r| height / r * factor),
        };
        if fov.width.is_some() || fov.height.is_some() {
            return Ok(Some(fov));
        }
    }
    // Explicit FOV declarations are distinct from TIFF's ordinary X/YResolution printing tags.
    let lower = desc.to_lowercase();
    if let Some(at) = lower.find("fov") {
        let tail = desc.get(at + 3..).unwrap_or("").trim_start_matches([' ', ':', '=']);
        let mut chars = tail.char_indices().peekable();
        let mut values = Vec::new();
        let mut end = 0;
        while values.len() < 2 {
            while chars.peek().is_some_and(|(_, c)| !c.is_ascii_digit() && !matches!(c, '.' | '-' | '+')) {
                chars.next();
            }
            let Some((start, _)) = chars.peek().copied() else { break };
            end = start;
            while let Some((i, c)) = chars.peek().copied() {
                if c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-') {
                    end = i + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            if let Some(value) = tail.get(start..end).and_then(|s| s.parse::<f64>().ok()).filter(|n| n.is_finite() && *n > 0.) {
                values.push(value);
            } else {
                break;
            }
        }
        if values.len() == 2 {
            let label =
                tail.get(end..).unwrap_or("").trim_start().split(|c: char| c.is_whitespace() || matches!(c, ';' | '<' | '\0')).next().unwrap_or("");
            if let Ok(f) = unit(label) {
                return Ok(Some(Fov { width: values.first().map(|n| n * f), height: values.get(1).map(|n| n * f) }));
            }
        }
    }
    Ok(None)
}

pub(super) fn read(path: &str) -> Result<Option<Fov>, String> {
    vectorcraft_engine::file_access::check_read(path).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let header = bytes(&mut file, 0, 8)?;
    let little = match header.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return Err("文件不是 TIFF".into()),
    };
    let version = integer(header.get(2..4).ok_or("缺少 TIFF 版本")?, little);
    let (offset, count_size, entry_size, value_size) = match version {
        42 => (integer(header.get(4..8).ok_or("缺少 TIFF 图像目录")?, little), 2, 12, 4),
        43 => {
            if integer(header.get(4..6).ok_or("缺少 BigTIFF 尺寸")?, little) != 8 {
                return Err("不支持该 BigTIFF 偏移格式".into());
            }
            (integer(&bytes(&mut file, 8, 8)?, little), 8, 20, 8)
        }
        _ => return Err("不支持该 TIFF 版本".into()),
    };
    let count = integer(&bytes(&mut file, offset, count_size)?, little);
    if count > 4096 {
        return Err("TIFF 字段数量过多".into());
    }
    let mut width = None;
    let mut height = None;
    let mut xres = None;
    let mut yres = None;
    let mut description = String::new();
    for index in 0..count {
        let at = offset.checked_add(count_size as u64).and_then(|p| p.checked_add(index * entry_size as u64)).ok_or("TIFF 偏移溢出")?;
        let entry = bytes(&mut file, at, entry_size)?;
        let tag = integer(entry.get(..2).ok_or("TIFF 标签不完整")?, little);
        if ![256, 257, 270, 282, 283].contains(&tag) {
            continue;
        }
        let kind = integer(entry.get(2..4).ok_or("TIFF 类型字段不完整")?, little);
        let count_end = if version == 43 { 12 } else { 8 };
        let n = integer(entry.get(4..count_end).ok_or("TIFF 计数字段不完整")?, little);
        let size = match kind {
            1 | 2 => 1,
            3 => 2,
            4 => 4,
            5 | 16 => 8,
            _ => continue,
        };
        let total = n.checked_mul(size).filter(|n| *n <= 1_048_576).ok_or("TIFF 字段过大")? as usize;
        let value = entry.get(count_end..).ok_or("TIFF 值字段不完整")?;
        let data = if total <= value_size {
            value.get(..total).ok_or("TIFF 内联字段不完整")?.to_vec()
        } else {
            bytes(&mut file, integer(value, little), total)?
        };
        if tag == 270 && kind == 2 {
            description = match std::str::from_utf8(&data) {
                Ok(s) => s.to_string(),
                Err(_) if data.starts_with(b"ImageJ=") => data.iter().map(|b| char::from(*b)).collect(),
                Err(_) => return Err("TIFF 元数据编码无效".into()),
            }
            .trim_end_matches('\0')
            .to_string();
            continue;
        }
        let number = if kind == 5 && data.len() >= 8 {
            let a = integer(data.get(..4).ok_or("TIFF 有理数字段不完整")?, little) as f64;
            let b = integer(data.get(4..8).ok_or("TIFF 有理数字段不完整")?, little) as f64;
            if b == 0. {
                continue;
            }
            a / b
        } else {
            let data = data.get(..size as usize).ok_or("TIFF 数值不完整")?;
            integer(data, little) as f64
        };
        match tag {
            256 => width = Some(number),
            257 => height = Some(number),
            282 => xres = Some(number),
            283 => yres = Some(number),
            _ => (),
        }
    }
    let width = width.filter(|n| *n > 0.).ok_or("缺少 TIFF 宽度")?;
    let height = height.filter(|n| *n > 0.).ok_or("缺少 TIFF 高度")?;
    let fov = metadata(&description, width, height, xres, yres)?;
    if fov.as_ref().is_some_and(|f| [f.width, f.height].into_iter().flatten().any(|n| !n.is_finite() || n <= 0.)) {
        return Err("物理标定数值无效".into());
    }
    Ok(fov)
}

pub(super) fn image_fov(
    app: &mut VectorcraftApp,
    b: &super::BoxInfo,
    nodes: &std::collections::BTreeMap<u64, serde_json::Value>,
) -> Result<Fov, String> {
    use serde_json::{Value, json};
    fn image_ids(n: &Value, out: &mut Vec<u64>) {
        if n.get("kind").and_then(Value::as_str) == Some("Image")
            && let Some(id) = n.get("id").and_then(Value::as_u64)
        {
            out.push(id);
        }
        for c in n.get("children").and_then(Value::as_array).into_iter().flatten() {
            image_ids(c, out);
        }
    }
    let mut ids = Vec::new();
    image_ids(nodes.get(&b.id).ok_or("找不到源对象")?, &mut ids);
    if ids.is_empty() || ids.len() > 7 {
        return Err("请选择链接 TIFF 或最多包含七个通道的组合".into());
    }
    let links = app.run("links.list", json!({}))?;
    let mut result: Option<Fov> = None;
    for id in ids {
        let link = links
            .get("links")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|l| l.get("id").and_then(Value::as_u64) == Some(id))
            .ok_or("找不到图片链接")?;
        let path = link.get("found").or_else(|| link.get("path")).and_then(Value::as_str).ok_or("嵌入图片没有原始元数据，请手动输入真实视野")?;
        let ext = std::path::Path::new(path).extension().and_then(|s| s.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("tif") && !ext.eq_ignore_ascii_case("tiff") {
            return Err("自动标定仅支持链接的 TIFF 文件".into());
        }
        let mut fov = read(path)?.ok_or("没有明确的显微标定，请手动输入真实视野")?;

        let st = app.session.active().ok_or("没有源文档")?;
        let frame = st.doc.oriented_bounds(&[vectorcraft_doc::NodeId(b.id)], false).ok_or("找不到源图框架")?;
        let image = st.doc.node(vectorcraft_doc::NodeId(id)).ok_or("找不到源图片")?;
        let vectorcraft_doc::NodeKind::Image(image) = &image.kind else { return Err("源对象不是位图".into()) };
        if image.width == 0 || image.height == 0 {
            return Err("图片像素尺寸无效".into());
        }
        let inverse = image.xf.inverse().as_coeffs();
        let (sin, cos) = (-frame.angle.to_radians()).sin_cos();
        let measure = |x: f64, y: f64| -> Result<Option<f64>, String> {
            let dx = inverse[0] * x + inverse[2] * y;
            let dy = inverse[1] * x + inverse[3] * y;
            if !dx.is_finite() || !dy.is_finite() {
                return Err("源对象变换不可逆".into());
            }
            if dx.abs() > 1e-7 && fov.width.is_none() || dy.abs() > 1e-7 && fov.height.is_none() {
                return Ok(None);
            }
            Ok(Some(
                ((dx * fov.width.unwrap_or(0.) / f64::from(image.width)).powi(2) + (dy * fov.height.unwrap_or(0.) / f64::from(image.height)).powi(2))
                    .sqrt(),
            ))
        };
        let width = measure(cos * frame.rect.width(), sin * frame.rect.width())?;
        let height = measure(-sin * frame.rect.height(), cos * frame.rect.height())?;
        fov.width = width;
        fov.height = height;

        if let Some(old) = &result {
            let same = |a: Option<f64>, b: Option<f64>| match (a, b) {
                (Some(a), Some(b)) => (a - b).abs() <= 1e-6 * a.abs().max(1.),
                (None, None) => true,
                _ => false,
            };
            if !same(old.width, fov.width) || !same(old.height, fov.height) {
                return Err("通道标定不一致，请明确输入真实视野".into());
            }
        } else {
            result = Some(fov);
        }
    }
    result.ok_or("没有找到实际标定".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_microscopy_metadata_is_required() {
        assert!(metadata("", 512., 256., Some(300.), Some(300.)).unwrap().is_none());
        let f = metadata("<Pixels PhysicalSizeX=\"0.5\" PhysicalSizeXUnit=\"µm\" PhysicalSizeY=\"1\"/>", 512., 256., None, None).unwrap().unwrap();
        assert_eq!(f.width, Some(256.));
        assert_eq!(f.height, Some(256.));
        let f = metadata("ImageJ=1.54\nunit=um", 512., 256., Some(2.), Some(4.)).unwrap().unwrap();
        assert_eq!(f.width, Some(256.));
        assert_eq!(f.height, Some(64.));
        let f = metadata("FOV: 120 x 60 um", 512., 256., None, None).unwrap().unwrap();
        assert_eq!(f.width, Some(120.));
    }
    #[test]
    fn bounded_reader_handles_classic_and_big_tiff_in_both_byte_orders() {
        fn number(n: u64, size: usize, little: bool) -> Vec<u8> {
            if little { n.to_le_bytes()[..size].to_vec() } else { n.to_be_bytes()[8 - size..].to_vec() }
        }
        for little in [true, false] {
            for big in [true, false] {
                let description = b"ImageJ=1.54\nunit=um\0";
                let header = if big { 16 } else { 8 };
                let count_bytes = if big { 8 } else { 2 };
                let entry_size = if big { 20 } else { 12 };
                let slot = if big { 8 } else { 4 };
                let base = header + count_bytes + 5 * entry_size + slot;
                let mut data = if little { b"II".to_vec() } else { b"MM".to_vec() };
                data.extend(number(if big { 43 } else { 42 }, 2, little));
                if big {
                    data.extend(number(8, 2, little));
                    data.extend(number(0, 2, little));
                }
                data.extend(number(header as u64, slot, little));
                data.extend(number(5, count_bytes, little));
                for (tag, kind, n, value) in [
                    (256, 4, 1, 512),
                    (257, 4, 1, 256),
                    (270, 2, description.len() as u64, base as u64),
                    (282, 5, 1, (base + description.len()) as u64),
                    (283, 5, 1, (base + description.len() + 8) as u64),
                ] {
                    data.extend(number(tag, 2, little));
                    data.extend(number(kind, 2, little));
                    data.extend(number(n, if big { 8 } else { 4 }, little));
                    let inline = if kind == 4 {
                        let mut v = number(value, 4, little);
                        v.resize(slot, 0);
                        v
                    } else if kind == 5 && big {
                        let mut v = number(if tag == 282 { 2 } else { 4 }, 4, little);
                        v.extend(number(1, 4, little));
                        v
                    } else {
                        number(value, slot, little)
                    };
                    data.extend(inline);
                }
                data.extend(number(0, slot, little));
                data.extend(description);
                for numerator in [2, 4] {
                    data.extend(number(numerator, 4, little));
                    data.extend(number(1, 4, little));
                }
                let path = std::env::temp_dir().join(format!("vectorcraft-sci-tiff-{}-{}-{}.tif", std::process::id(), little, big));
                std::fs::write(&path, &data).unwrap();
                let f = read(path.to_str().unwrap()).unwrap().unwrap();
                std::fs::remove_file(path).unwrap();
                assert_eq!(f.width, Some(256.));
                assert_eq!(f.height, Some(64.));
            }
        }
    }
}
