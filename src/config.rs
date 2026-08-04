use std::fs;
use serde_json::Value;

fn kdl_to_json(doc: &kdl::KdlDocument) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for node in doc.nodes() {
        let name = node.name().value().to_string();
        
        let mut node_map = serde_json::Map::new();
        let mut has_props = false;
        for entry in node.entries() {
            if let Some(prop_name) = entry.name() {
                has_props = true;
                let j_val = match entry.value() {
                    kdl::KdlValue::Bool(b) => serde_json::Value::Bool(*b),
                    kdl::KdlValue::Base2(i) |
                    kdl::KdlValue::Base8(i) |
                    kdl::KdlValue::Base10(i) |
                    kdl::KdlValue::Base16(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
                    kdl::KdlValue::Base10Float(f) => {
                        let mut val_f = *f;
                        if let Some(ty) = entry.ty() {
                            let ty_str = ty.value();
                            if ty_str.starts_with("f64:") {
                                let range_str = ty_str.trim_start_matches("f64:");
                                if let Some(dash_idx) = range_str.find('-') {
                                    let min_str = &range_str[..dash_idx].trim();
                                    let max_str = &range_str[dash_idx + 1..].trim();
                                    if let (Ok(min_f), Ok(max_f)) = (min_str.parse::<f64>(), max_str.parse::<f64>()) {
                                        val_f = val_f.clamp(min_f, max_f);
                                    }
                                }
                            }
                        }
                        if let Some(num) = serde_json::Number::from_f64(val_f) {
                            serde_json::Value::Number(num)
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    kdl::KdlValue::String(s) |
                    kdl::KdlValue::RawString(s) => serde_json::Value::String(s.clone()),
                    kdl::KdlValue::Null => serde_json::Value::Null,
                };
                node_map.insert(prop_name.value().to_string(), j_val);
            }
        }

        let val = if name == "key_bindings" {
            if let Some(children) = node.children() {
                let mut binds = Vec::new();
                for child in children.nodes() {
                    let mut child_map = serde_json::Map::new();
                    for entry in child.entries() {
                        if let Some(prop_name) = entry.name() {
                            let j_val = match entry.value() {
                                kdl::KdlValue::Bool(b) => serde_json::Value::Bool(*b),
                                kdl::KdlValue::Base2(i) |
                                kdl::KdlValue::Base8(i) |
                                kdl::KdlValue::Base10(i) |
                                kdl::KdlValue::Base16(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
                                kdl::KdlValue::Base10Float(f) => {
                                    let mut val_f = *f;
                                    if let Some(ty) = entry.ty() {
                                        let ty_str = ty.value();
                                        if ty_str.starts_with("f64:") {
                                            let range_str = ty_str.trim_start_matches("f64:");
                                            if let Some(dash_idx) = range_str.find('-') {
                                                let min_str = &range_str[..dash_idx].trim();
                                                let max_str = &range_str[dash_idx + 1..].trim();
                                                if let (Ok(min_f), Ok(max_f)) = (min_str.parse::<f64>(), max_str.parse::<f64>()) {
                                                    val_f = val_f.clamp(min_f, max_f);
                                                }
                                            }
                                        }
                                    }
                                    if let Some(num) = serde_json::Number::from_f64(val_f) {
                                        serde_json::Value::Number(num)
                                    } else {
                                        serde_json::Value::Null
                                    }
                                }
                                kdl::KdlValue::String(s) |
                                kdl::KdlValue::RawString(s) => serde_json::Value::String(s.clone()),
                                kdl::KdlValue::Null => serde_json::Value::Null,
                            };
                            child_map.insert(prop_name.value().to_string(), j_val);
                        }
                    }
                    binds.push(serde_json::Value::Object(child_map));
                }
                serde_json::Value::Array(binds)
            } else if has_props {
                serde_json::Value::Object(node_map)
            } else {
                serde_json::Value::Null
            }
        } else {
            let mut node_val = serde_json::Value::Null;
            if has_props {
                node_val = serde_json::Value::Object(node_map);
            } else if node.entries().len() > 1 {
                let parts: Vec<String> = node.entries().iter().map(|entry| {
                    match entry.value() {
                        kdl::KdlValue::Bool(b) => b.to_string(),
                        kdl::KdlValue::Base2(i) |
                        kdl::KdlValue::Base8(i) |
                        kdl::KdlValue::Base10(i) |
                        kdl::KdlValue::Base16(i) => i.to_string(),
                        kdl::KdlValue::Base10Float(f) => f.to_string(),
                        kdl::KdlValue::String(s) |
                        kdl::KdlValue::RawString(s) => s.clone(),
                        kdl::KdlValue::Null => "null".to_string(),
                    }
                }).collect();
                node_val = serde_json::Value::String(parts.join(" "));
            } else if let Some(entry) = node.entries().first() {
                node_val = match entry.value() {
                    kdl::KdlValue::Bool(b) => serde_json::Value::Bool(*b),
                    kdl::KdlValue::Base2(i) |
                    kdl::KdlValue::Base8(i) |
                    kdl::KdlValue::Base10(i) |
                    kdl::KdlValue::Base16(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
                    kdl::KdlValue::Base10Float(f) => {
                        let mut val_f = *f;
                        if let Some(ty) = entry.ty() {
                            let ty_str = ty.value();
                            if ty_str.starts_with("f64:") {
                                let range_str = ty_str.trim_start_matches("f64:");
                                if let Some(dash_idx) = range_str.find('-') {
                                    let min_str = &range_str[..dash_idx].trim();
                                    let max_str = &range_str[dash_idx + 1..].trim();
                                    if let (Ok(min_f), Ok(max_f)) = (min_str.parse::<f64>(), max_str.parse::<f64>()) {
                                        val_f = val_f.clamp(min_f, max_f);
                                    }
                                }
                            }
                        }
                        if let Some(num) = serde_json::Number::from_f64(val_f) {
                            serde_json::Value::Number(num)
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    kdl::KdlValue::String(s) |
                    kdl::KdlValue::RawString(s) => serde_json::Value::String(s.clone()),
                    kdl::KdlValue::Null => serde_json::Value::Null,
                };
            }

            if let Some(children) = node.children() {
                let children_val = kdl_to_json(children);
                if let serde_json::Value::Object(children_map) = children_val {
                    if let serde_json::Value::Object(mut nm) = node_val {
                        for (k, v) in children_map {
                            nm.insert(k, v);
                        }
                        serde_json::Value::Object(nm)
                    } else {
                        serde_json::Value::Object(children_map)
                    }
                } else {
                    node_val
                }
            } else {
                node_val
            }
        };

        if let Some(existing) = map.remove(&name) {
            match existing {
                serde_json::Value::Array(mut arr) => {
                    match val {
                        serde_json::Value::Array(new_arr) => {
                            arr.extend(new_arr);
                        }
                        _ => {
                            arr.push(val);
                        }
                    }
                    map.insert(name, serde_json::Value::Array(arr));
                }
                other => {
                    match val {
                        serde_json::Value::Array(new_arr) => {
                            let mut combined = vec![other];
                            combined.extend(new_arr);
                            map.insert(name, serde_json::Value::Array(combined));
                        }
                        _ => {
                            map.insert(name, serde_json::Value::Array(vec![other, val]));
                        }
                    }
                }
            }
        } else {
            let list_names = ["key_bindings", "pointer_bind", "gesture_bind", "mode_rule", "tag_layout", "startup", "device"];
            if list_names.contains(&name.as_str()) {
                match val {
                    serde_json::Value::Array(_) => {
                        map.insert(name, val);
                    }
                    _ => {
                        map.insert(name, serde_json::Value::Array(vec![val]));
                    }
                }
            } else {
                map.insert(name, val);
            }
        }
    }
    serde_json::Value::Object(map)
}

pub fn get_app_name() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.file_name()
                .and_then(|s| s.to_str().map(|ss| ss.to_string()))
        })
}

pub fn get_app_config_path(app_name: &str) -> std::path::PathBuf {
    get_config_path().parent().unwrap().join(app_name).join("config.kdl")
}

fn merge_json(a: &mut serde_json::Value, b: &serde_json::Value) {
    match (a, b) {
        (serde_json::Value::Object(a_map), serde_json::Value::Object(b_map)) => {
            for (k, v) in b_map {
                if !v.is_null() {
                    merge_json(a_map.entry(k.clone()).or_insert(serde_json::Value::Null), v);
                }
            }
        }
        (a_val, b_val) => {
            *a_val = b_val.clone();
        }
    }
}

pub fn parse_kdl_to_json(content: &str) -> serde_json::Value {
    let mut main_val = if let Ok(doc) = content.parse::<kdl::KdlDocument>() {
        kdl_to_json(&doc)
    } else {
        serde_json::json!({})
    };

    if let Some(app_name) = get_app_name() {
        let app_path = get_app_config_path(&app_name);
        if let Ok(override_content) = std::fs::read_to_string(&app_path) {
            if let Ok(override_doc) = override_content.parse::<kdl::KdlDocument>() {
                let override_val = kdl_to_json(&override_doc);
                merge_json(&mut main_val, &override_val);
            }
        }
    }

    main_val
}

pub fn update_json_in_memory(val_obj: &mut Value, key: &str, value: &str, default_section: &str) -> bool {
    let j_val = if let Ok(parsed_val) = serde_json::from_str::<Value>(value) {
        parsed_val
    } else {
        serde_json::json!(value)
    };

    let mut updated = false;
    if let Some(obj) = val_obj.as_object_mut() {
        for (_sec_name, sec_val) in obj.iter_mut() {
            if let Some(sec_obj) = sec_val.as_object_mut() {
                if sec_obj.contains_key(key) {
                    sec_obj.insert(key.to_string(), j_val.clone());
                    updated = true;
                    break;
                }
            }
        }
        if !updated {
            if let Some(sec_obj) = obj.get_mut(default_section).and_then(|s| s.as_object_mut()) {
                sec_obj.insert(key.to_string(), j_val);
                updated = true;
            } else {
                let mut map = serde_json::Map::new();
                map.insert(key.to_string(), j_val);
                obj.insert(default_section.to_string(), Value::Object(map));
                updated = true;
            }
        }
    }
    updated
}

pub fn parse_config_path(key: &str, default_section: &str) -> (String, String, Option<String>) {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() == 3 {
        (parts[0].to_string(), parts[1].to_string(), Some(parts[2].to_string()))
    } else if parts.len() == 2 {
        (parts[0].to_string(), parts[1].to_string(), None)
    } else {
        (default_section.to_string(), key.to_string(), None)
    }
}

const PROP_NODES: &[&str] = &[
    "gestures", "key_bindings", "pointer_bind", "gesture_bind",
    "button", "button_strip", "dropdown", "toggle", "spinbox", "slider", "font_selector",
    "status", "overlay", "backplate", "desktop", "list", "section", "textbox", "multiline", "editor", "tree",
    "menubar", "statusbar", "node", "relief"
];

fn get_or_create_node_mut<'a>(doc: &'a mut kdl::KdlDocument, path: &[&str]) -> Option<&'a mut kdl::KdlNode> {
    if path.is_empty() {
        return None;
    }
    let segment = path[0];
    let idx = if let Some(i) = doc.nodes().iter().position(|n| n.name().value() == segment) {
        i
    } else {
        let new_node = format!("{}\n", segment).parse::<kdl::KdlNode>().ok()?;
        doc.nodes_mut().push(new_node);
        doc.nodes().len() - 1
    };
    if path.len() == 1 {
        Some(&mut doc.nodes_mut()[idx])
    } else {
        let children = doc.nodes_mut()[idx].ensure_children();
        get_or_create_node_mut(children, &path[1..])
    }
}

fn get_node_ref<'a>(doc: &'a kdl::KdlDocument, path: &[&str]) -> Option<&'a kdl::KdlNode> {
    if path.is_empty() {
        return None;
    }
    let segment = path[0];
    let node = doc.nodes().iter().find(|n| n.name().value() == segment)?;
    if path.len() == 1 {
        Some(node)
    } else {
        let children = node.children()?;
        get_node_ref(children, &path[1..])
    }
}

pub fn update_kdl_in_memory(doc: &mut kdl::KdlDocument, key: &str, value: &str, default_section: &str) -> bool {
    update_kdl_in_memory_typed(doc, key, value, default_section, None)
}

/// [`update_kdl_in_memory`] with an explicit type annotation for the written
/// entry. `forced_ty` overrides both the value-shape inference and the
/// preserved existing annotation — how a writer ESTABLISHES a custom type
/// (e.g. cce-bevel writing `(bevel)` knob keys into a config that never had
/// them; preservation alone can't create the annotation).
pub fn update_kdl_in_memory_typed(doc: &mut kdl::KdlDocument, key: &str, value: &str, _default_section: &str, forced_ty: Option<&str>) -> bool {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return false;
    }

    let is_property = parts.len() >= 2 && PROP_NODES.contains(&parts[parts.len() - 2]);

    let (node_path, target_prop) = if is_property {
        (&parts[0..parts.len() - 1], Some(parts[parts.len() - 1].to_string()))
    } else {
        (&parts[0..parts.len()], None)
    };

    let child_node = if let Some(node) = get_or_create_node_mut(doc, node_path) {
        node
    } else {
        return false;
    };

    let existing_ty = if let Some(ref prop_name) = target_prop {
        child_node.entries().iter()
            .find(|e| e.name().map(|n| n.value()) == Some(prop_name))
            .and_then(|e| e.ty().map(|t| t.value().to_string()))
    } else {
        child_node.entries().first()
            .and_then(|e| e.ty().map(|t| t.value().to_string()))
    };

    let (kdl_val, mut kdl_ty) = if let Ok(b) = value.parse::<bool>() {
        (kdl::KdlValue::Bool(b), Some("bool".to_string()))
    } else if value.starts_with('#') {
        let s_clean = value.trim_start_matches('#');
        let ty = if s_clean.len() == 8 { "rgba" } else { "rgb" };
        (kdl::KdlValue::String(value.to_string()), Some(ty.to_string()))
    } else if value.contains('.') {
        if let Ok(f) = value.parse::<f64>() {
            (kdl::KdlValue::Base10Float(f), Some("f64".to_string()))
        } else {
            (kdl::KdlValue::String(value.to_string()), None)
        }
    } else if let Ok(i) = value.parse::<i64>() {
        (kdl::KdlValue::Base10(i), Some("i64".to_string()))
    } else {
        let s = value.trim_matches('"').to_string();
        (kdl::KdlValue::String(s), None)
    };

    if let Some(ref ext_ty) = existing_ty {
        if ext_ty.starts_with("menu:") || ext_ty == "button" || ext_ty.starts_with("button:") || ext_ty == "vec2i" || ext_ty == "radian" || ext_ty == "bevel" || ext_ty == "keybind" {
            kdl_ty = Some(ext_ty.clone());
        }
    }
    if let Some(f) = forced_ty {
        kdl_ty = Some(f.to_string());
    }

    if let Some(prop_name) = target_prop {
        let mut found = false;
        for entry in child_node.entries_mut() {
            if let Some(id) = entry.name() {
                if id.value() == prop_name {
                    *entry = kdl::KdlEntry::new_prop(prop_name.clone(), kdl_val.clone());
                    if let Some(ref ty) = kdl_ty {
                        entry.set_ty(ty.as_str());
                    }
                    found = true;
                    break;
                }
            }
        }
        if !found {
            let mut entry = kdl::KdlEntry::new_prop(prop_name, kdl_val);
            if let Some(ref ty) = kdl_ty {
                entry.set_ty(ty.as_str());
            }
            child_node.entries_mut().push(entry);
        }
    } else {
        child_node.entries_mut().clear();
        if kdl_ty.as_deref() == Some("vec2i") {
            let parts: Vec<&str> = value.split_whitespace().collect();
            for (idx, part) in parts.iter().enumerate() {
                if let Ok(i) = part.parse::<i64>() {
                    let mut entry = kdl::KdlEntry::new(kdl::KdlValue::Base10(i));
                    if idx == 0 {
                        entry.set_ty("vec2i");
                    }
                    child_node.entries_mut().push(entry);
                }
            }
        } else {
            let mut entry = kdl::KdlEntry::new(kdl_val);
            if let Some(ref ty) = kdl_ty {
                entry.set_ty(ty.as_str());
            }
            child_node.entries_mut().push(entry);
        }
    }

    true
}

/// XDG config base directory: `$XDG_CONFIG_HOME`, else `~/.config`.
pub fn config_home() -> std::path::PathBuf {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(x) if !x.is_empty() => std::path::PathBuf::from(x),
        _ => std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"),
    }
}

/// XDG data base directory: `$XDG_DATA_HOME`, else `~/.local/share`.
pub fn data_home() -> std::path::PathBuf {
    match std::env::var("XDG_DATA_HOME") {
        Ok(x) if !x.is_empty() => std::path::PathBuf::from(x),
        _ => std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join(".local")
            .join("share"),
    }
}

/// The cce config directory (`<config_home>/cce`).
pub fn cce_config_dir() -> std::path::PathBuf {
    config_home().join("cce")
}

pub fn get_config_path() -> std::path::PathBuf {
    cce_config_dir().join("config.kdl")
}

struct ConfigCache {
    last_modified: Option<std::time::SystemTime>,
    parsed: Option<serde_json::Value>,
    raw_content: String,
}

static CONFIG_CACHE: std::sync::RwLock<ConfigCache> = std::sync::RwLock::new(ConfigCache {
    last_modified: None,
    parsed: None,
    raw_content: String::new(),
});

/// The cce config parsed to JSON, cached on the config file's mtime (per process).
/// Re-reads and re-parses only when `get_config_path()`'s modification time changes.
pub fn cached_config() -> serde_json::Value {
    let path = get_config_path();
    let current_modified = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());

    if let Ok(cache) = CONFIG_CACHE.read() {
        if cache.last_modified.is_some() && cache.last_modified == current_modified {
            if let Some(ref val) = cache.parsed {
                return val.clone();
            }
        }
    }

    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let val = parse_kdl_to_json(&content);
    if let Ok(mut cache) = CONFIG_CACHE.write() {
        cache.last_modified = current_modified;
        cache.parsed = Some(val.clone());
        cache.raw_content = content;
    }
    val
}

/// The raw text of the cce config, cached alongside [`cached_config`].
pub fn cached_config_content() -> String {
    let _ = cached_config();
    CONFIG_CACHE.read().map(|c| c.raw_content.clone()).unwrap_or_default()
}

// ── Typed accessors over the cached config ──────────────────────────────────
// Each reads the mtime-cached config and extracts a value at a JSON pointer
// (e.g. "/notifications/enable"), returning the default when absent or mistyped.

/// Read a boolean at `pointer` from the cached config, or `default`.
pub fn get_bool(pointer: &str, default: bool) -> bool {
    cached_config().pointer(pointer).and_then(|v| v.as_bool()).unwrap_or(default)
}

/// Read an f32 at `pointer` from the cached config, or `default`.
pub fn get_f32(pointer: &str, default: f32) -> f32 {
    cached_config()
        .pointer(pointer)
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(default)
}

/// Read an i64 at `pointer` from the cached config, or `default`.
pub fn get_i64(pointer: &str, default: i64) -> i64 {
    cached_config().pointer(pointer).and_then(|v| v.as_i64()).unwrap_or(default)
}

/// Read a string at `pointer` from the cached config.
pub fn get_string(pointer: &str) -> Option<String> {
    cached_config().pointer(pointer).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Read a hex color string at `pointer` and parse it to raw sRGB RGBA (`[0,1]`).
/// Apply [`crate::color::srgb_to_linear`] if your render target expects linear.
pub fn get_color(pointer: &str) -> Option<[f32; 4]> {
    get_string(pointer).as_deref().and_then(crate::color::parse_hex_rgba)
}

/// Recursively search a JSON value for the first entry whose object key equals
/// `key`, returning a reference to its value. Depth-first over objects and arrays.
pub fn find_key<'a>(val: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    match val {
        serde_json::Value::Object(map) => {
            if let Some(found) = map.get(key) {
                return Some(found);
            }
            for v in map.values() {
                if let Some(found) = find_key(v, key) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => arr.iter().find_map(|v| find_key(v, key)),
        _ => None,
    }
}



fn perform_rolling_backup(path: &str) {
    let config_path = get_config_path();
    if std::path::Path::new(path) != config_path {
        return;
    }
    if !std::path::Path::new(path).exists() {
        return;
    }
    let backup_dir = config_path.parent().unwrap().join("backups");
    if let Err(_) = fs::create_dir_all(&backup_dir) {
        return;
    }
    for i in (1..=4).rev() {
        let src = backup_dir.join(format!("config.kdl.{}.bak", i));
        let dst = backup_dir.join(format!("config.kdl.{}.bak", i + 1));
        if src.exists() {
            let _ = fs::rename(src, dst);
        }
    }
    let dst = backup_dir.join("config.kdl.1.bak");
    let _ = fs::copy(path, dst);
}

pub(crate) fn safe_write(path: &str, content: &str) -> bool {
    perform_rolling_backup(path);
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let temp_path = format!("{}.tmp", path);
    if fs::write(&temp_path, content).is_ok() {
        if fs::rename(&temp_path, path).is_ok() {
            return true;
        }
        let _ = fs::remove_file(&temp_path);
    }
    false
}

pub fn write_config_value(path: &str, key: &str, value: &str, default_section: &str) -> bool {
    write_config_value_typed(path, key, value, default_section, None)
}

/// [`write_config_value`] with an explicit type annotation — see
/// [`update_kdl_in_memory_typed`].
pub fn write_config_value_typed(path: &str, key: &str, value: &str, default_section: &str, forced_ty: Option<&str>) -> bool {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut doc = match content.parse::<kdl::KdlDocument>() {
        Ok(d) => d,
        Err(_) => kdl::KdlDocument::new(),
    };

    if update_kdl_in_memory_typed(&mut doc, key, value, default_section, forced_ty) {
        let updated_str = doc.to_string();
        return safe_write(path, &updated_str);
    }
    false
}

pub fn get_kdl_type_annotation(kdl_content: &str, key_path: &str) -> Option<String> {
    let doc: kdl::KdlDocument = kdl_content.parse().ok()?;
    let parts: Vec<&str> = key_path.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    let is_property = parts.len() >= 2 && PROP_NODES.contains(&parts[parts.len() - 2]);

    let (node_path, target_prop) = if is_property {
        (&parts[0..parts.len() - 1], Some(parts[parts.len() - 1].to_string()))
    } else {
        (&parts[0..parts.len()], None)
    };

    let child_node = get_node_ref(&doc, node_path)?;

    if let Some(prop_name) = target_prop {
        let entry = child_node.entries().iter().find(|e| e.name().map(|n| n.value()) == Some(&prop_name))?;
        entry.ty().map(|t| t.value().to_string())
    } else {
        let entry = child_node.entries().first()?;
        entry.ty().map(|t| t.value().to_string())
    }
}

pub fn get_kdl_type_annotations(kdl_content: &str, key_paths: &[String]) -> Vec<Option<String>> {
    let doc = match kdl_content.parse::<kdl::KdlDocument>() {
        Ok(d) => Some(d),
        Err(_) => None,
    };
    key_paths.iter().map(|key_path| {
        let doc = doc.as_ref()?;
        let parts: Vec<&str> = key_path.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        let is_property = parts.len() >= 2 && PROP_NODES.contains(&parts[parts.len() - 2]);

        let (node_path, target_prop) = if is_property {
            (&parts[0..parts.len() - 1], Some(parts[parts.len() - 1].to_string()))
        } else {
            (&parts[0..parts.len()], None)
        };

        let child_node = get_node_ref(doc, node_path)?;

        if let Some(prop_name) = target_prop {
            let entry = child_node.entries().iter().find(|e| e.name().map(|n| n.value()) == Some(&prop_name))?;
            entry.ty().map(|t| t.value().to_string())
        } else {
            let entry = child_node.entries().first()?;
            entry.ty().map(|t| t.value().to_string())
        }
    }).collect()
}


#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_nested_parsing() {
        let content = "style {\n    status box_opacity=(f64)0.75\n}\n";
        let val = parse_kdl_to_json(content);
        println!("val = {:?}", val);
        let (sec, node, prop) = parse_config_path("style.status.box_opacity", "layout");
        assert_eq!(sec, "style");
        assert_eq!(node, "status");
        assert_eq!(prop, Some("box_opacity".to_string()));
        
        let sec_val = val.get(&sec).unwrap();
        let node_val = sec_val.get(&node).unwrap();
        let prop_val = node_val.get(prop.as_ref().unwrap()).unwrap();
        assert_eq!(prop_val.as_f64().unwrap(), 0.75);
    }

    #[test]
    fn relief_keys_write_as_properties_and_round_trip() {
        // `relief` is a PROP_NODES member: style.surface.relief.* must land as
        // properties on the existing relief node (the config.kdl shape), not
        // as duplicate child nodes shadowing the depth=/width= properties.
        let content = "style {\n    surface {\n        relief depth=(f64)0.15 width=(f64)9.3\n    }\n}\n";
        let mut doc = content.parse::<kdl::KdlDocument>().unwrap();
        let spec = "smooth;0.000:0.500,0.400:1.000,1.000:0.000";
        assert!(update_kdl_in_memory(&mut doc, "style.surface.relief.profile", spec, "style"));
        assert!(update_kdl_in_memory(&mut doc, "style.surface.relief.depth", "0.3", "style"));
        let out = doc.to_string();
        // Still one relief node, no child block grown under it.
        assert_eq!(out.matches("relief").count(), 1, "out: {out}");
        assert!(!out.contains("relief {"), "out: {out}");

        // The reload path reads through parse_kdl_to_json: the new property
        // must surface at the same dotted path the style registry maps.
        let val = parse_kdl_to_json(&out);
        let relief = val.get("style").unwrap().get("surface").unwrap().get("relief").unwrap();
        assert_eq!(relief.get("profile").unwrap().as_str().unwrap(), spec);
        assert_eq!(relief.get("depth").unwrap().as_f64().unwrap(), 0.3);
        assert_eq!(relief.get("width").unwrap().as_f64().unwrap(), 9.3);
    }

    #[test]
    fn test_get_kdl_type_annotation() {
        let content = "input {\n    accel_profile (\"menu:flat,adaptive,none,custom\")\"flat\"\n    touchpad {\n        gestures pinch=(bool)true\n    }\n}\n";
        let ty1 = get_kdl_type_annotation(content, "input.accel_profile");
        assert_eq!(ty1, Some("menu:flat,adaptive,none,custom".to_string()));
        
        let ty2 = get_kdl_type_annotation(content, "input.touchpad.gestures.pinch");
        assert_eq!(ty2, Some("bool".to_string()));

        let keys = vec![
            "input.accel_profile".to_string(),
            "input.touchpad.gestures.pinch".to_string(),
            "input.invalid_key".to_string(),
        ];
        let tys = get_kdl_type_annotations(content, &keys);
        assert_eq!(tys.len(), 3);
        assert_eq!(tys[0], Some("menu:flat,adaptive,none,custom".to_string()));
        assert_eq!(tys[1], Some("bool".to_string()));
        assert_eq!(tys[2], None);
    }

    #[test]
    fn test_json_to_kdl_with_special_annotations() {
        let mut annotations = std::collections::HashMap::new();
        annotations.insert("style.surface.desktop.mode".to_string(), "menu:grid,solid".to_string());
        
        let mut desktop_map = serde_json::Map::new();
        desktop_map.insert("mode".to_string(), serde_json::Value::String("grid".to_string()));
        
        let mut surface_map = serde_json::Map::new();
        surface_map.insert("desktop".to_string(), serde_json::Value::Object(desktop_map));
        
        let mut style_map = serde_json::Map::new();
        style_map.insert("surface".to_string(), serde_json::Value::Object(surface_map));
        
        let mut root_map = serde_json::Map::new();
        root_map.insert("style".to_string(), serde_json::Value::Object(style_map));
        
        let root = serde_json::Value::Object(root_map);
        let kdl_str = json_to_kdl_string_with_annotations(&root, &annotations);
        println!("Generated KDL:\n{}", kdl_str);
        
        let doc_parsed = kdl_str.parse::<kdl::KdlDocument>();
        assert!(doc_parsed.is_ok(), "Failed to parse KDL: {:?}", doc_parsed.err());
    }

    #[test]
    fn test_brightness_annotations() {
        let mut edp_map = serde_json::Map::new();
        edp_map.insert("scale".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(2.0).unwrap()));
        edp_map.insert("brightness_up".to_string(), serde_json::Value::String("XF86MonBrightnessUp".to_string()));
        edp_map.insert("brightness_down".to_string(), serde_json::Value::String("XF86MonBrightnessDown".to_string()));
        edp_map.insert("brightness_interval".to_string(), serde_json::Value::Number(serde_json::Number::from(10)));

        let mut output_map = serde_json::Map::new();
        output_map.insert("eDP-1".to_string(), serde_json::Value::Object(edp_map));

        let mut root_map = serde_json::Map::new();
        root_map.insert("output".to_string(), serde_json::Value::Object(output_map));

        let root = serde_json::Value::Object(root_map);
        let kdl_str = json_to_kdl_string(&root);
        println!("Generated KDL for brightness:\n{}", kdl_str);

        let doc_parsed = kdl_str.parse::<kdl::KdlDocument>().unwrap();
        
        let output_node = doc_parsed.nodes().iter().find(|n| n.name().value() == "output").unwrap();
        let edp_node = output_node.children().unwrap().nodes().iter().find(|n| n.name().value() == "eDP-1").unwrap();
        
        let up_entry = edp_node.entries().iter().find(|e| e.name().map(|n| n.value()) == Some("brightness_up")).unwrap();
        assert_eq!(up_entry.ty().unwrap().value(), "keybind");

        let down_entry = edp_node.entries().iter().find(|e| e.name().map(|n| n.value()) == Some("brightness_down")).unwrap();
        assert_eq!(down_entry.ty().unwrap().value(), "keybind");

        let interval_entry = edp_node.entries().iter().find(|e| e.name().map(|n| n.value()) == Some("brightness_interval")).unwrap();
        assert_eq!(interval_entry.ty().unwrap().value(), "i64");
    }

    #[test]
    fn test_backplate_menubar_statusbar_styling() {
        // Trigger load_colors_once first to initialize the Once block from the real config file
        let _ = crate::color::backplate_statusbar_blur();

        let content = r##"
            style {
                surface {
                    backplate blur=(f64)0.1 color=(rgba)"#5e657acf" corner_radius=(i64)12 {
                        menubar blur=(bool)true color=(rgba)"#1a1d26d0" text_color=(rgba)"#e2e4f0ff"
                    }
                    statusbar blur=(bool)false color=(rgba)"#12141cd0" text_color=(rgba)"#b5b9c8ff"
                }
                control {
                    dropdown color=(rgba)"#08080cff"
                }
                data {
                    textbox placeholder_text_color=(rgba)"#60606aff"
                }
            }
        "##;
        
        // Parse into json and set colors
        crate::color::reload_colors(content);

        // Verify values are parsed correctly
        assert_eq!(crate::color::backplate_menubar_blur(), true);
        
        let dd_color = crate::color::dropdown_background_color();
        assert!((dd_color[0] - crate::color::srgb_to_linear(8.0 / 255.0)).abs() < 0.0001);
        
        let placeholder_color = crate::color::textbox_placeholder_text_color();
        assert_eq!(placeholder_color, [0x60, 0x60, 0x6a]);
        assert_eq!(crate::color::backplate_statusbar_blur(), false);

        // Colors are in sRGB converted to linear, let's verify text colors
        let menubar_txt = crate::color::backplate_menubar_text_color();
        assert!(menubar_txt[0] > 0.0);
        let statusbar_txt = crate::color::backplate_statusbar_text_color();
        assert!(statusbar_txt[0] > 0.0);
    }

    #[test]
    fn test_vec2i_lossless_roundtrip() {
        let content = "style {\n    surface {\n        cloud {\n            position_default (vec2i)100 200\n        }\n    }\n}\n";
        let val = parse_kdl_to_json(content);
        println!("Parsed KDL to JSON: {:?}", val);
        
        let position_default_val = val.get("style").unwrap()
            .get("surface").unwrap()
            .get("cloud").unwrap()
            .get("position_default").unwrap();
        assert_eq!(position_default_val.as_str().unwrap(), "100 200");

        let mut annotations = std::collections::HashMap::new();
        annotations.insert("style.surface.cloud.position_default".to_string(), "vec2i".to_string());
        
        let kdl_str = json_to_kdl_string_with_annotations(&val, &annotations);
        println!("Generated KDL:\n{}", kdl_str);
        
        // Assert that (vec2i)100 200 is preserved without quotes
        assert!(kdl_str.contains("position_default (vec2i)100 200"));
        
        // Test update_kdl_in_memory preserves and updates the KDL Document correctly
        let mut doc = kdl_str.parse::<kdl::KdlDocument>().unwrap();
        let updated = update_kdl_in_memory(&mut doc, "style.surface.cloud.position_default", "150 250", "layout");
        assert!(updated);
        let updated_kdl = doc.to_string();
        println!("Updated KDL:\n{}", updated_kdl);
        assert!(updated_kdl.contains("position_default (vec2i)150 250"));
    }
}

fn format_kdl_type(ty: &str) -> String {
    let is_ident = !ty.is_empty()
        && !ty.chars().next().unwrap().is_ascii_digit()
        && ty.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '?' | '!' | '@' | '*' | '~' | '|' | '.'));
    if is_ident {
        ty.to_string()
    } else {
        format!("\"{}\"", ty.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn format_kdl_identifier(name: &str) -> String {
    let is_ident = !name.is_empty()
        && !name.chars().next().unwrap().is_ascii_digit()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '?' | '!' | '@' | '*' | '~' | '|' | '.'));
    if is_ident {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

pub fn value_to_kdl(key: &str, val: &serde_json::Value, indent: usize) -> String {
    value_to_kdl_with_annotations(key, val, indent, "", &std::collections::HashMap::new())
}

pub fn value_to_kdl_with_annotations(
    key: &str,
    val: &serde_json::Value,
    indent: usize,
    parent_path: &str,
    annotations: &std::collections::HashMap<String, String>,
) -> String {
    let indent_str = "    ".repeat(indent);
    let current_path = if parent_path.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", parent_path, key)
    };

    match val {
        serde_json::Value::Object(map) => {
            let has_objects = map.values().any(|v| v.is_object());
            if has_objects {
                let mut out = format!("{}{} {{\n", indent_str, format_kdl_identifier(key));
                for (k, v) in map {
                    out.push_str(&value_to_kdl_with_annotations(k, v, indent + 1, &current_path, annotations));
                }
                out.push_str(&format!("{}}}\n", indent_str));
                out
            } else {
                let mut prop_parts = Vec::new();
                let mut child_parts = Vec::new();
                for (prop_name, prop_val) in map {
                    let prop_path = format!("{}.{}", current_path, prop_name);
                    let is_vec2i = annotations.get(&prop_path).map_or(false, |a| a == "vec2i");
                    if is_vec2i {
                        if let serde_json::Value::String(ref s) = prop_val {
                            child_parts.push(format!("{}{} (vec2i){}\n", "    ".repeat(indent + 1), prop_name, s));
                        }
                    } else {
                        let (val_str, val_ty) = match prop_val {
                            serde_json::Value::Bool(b) => (b.to_string(), Some("bool".to_string())),
                            serde_json::Value::Number(num) => {
                                if prop_name == "light_source_position" {
                                    (num.to_string(), Some("radian".to_string()))
                                } else if num.is_f64() {
                                    (num.to_string(), Some("f64".to_string()))
                                } else {
                                    (num.to_string(), Some("i64".to_string()))
                                }
                            }
                            serde_json::Value::String(s) => {
                                if let Some(anno) = annotations.get(&prop_path) {
                                    if anno == "vec2i" {
                                        (s.clone(), Some(anno.clone()))
                                    } else {
                                        (format!("\"{}\"", s), Some(anno.clone()))
                                    }
                                } else if s.starts_with('#') {
                                    let s_clean = s.trim_start_matches('#');
                                    let ty = if s_clean.len() == 8 { "rgba" } else { "rgb" };
                                    (format!("\"{}\"", s), Some(ty.to_string()))
                                } else if prop_name == "key" || prop_name == "keybind" || prop_name == "shortcut" || prop_name == "open_search" || prop_name == "close_search" || prop_name == "delete" || prop_name.ends_with("_key") || prop_name.ends_with(".key") || prop_name.ends_with(".keybind") || prop_name.ends_with(".open_search") || prop_name.ends_with(".close_search") || prop_name == "brightness_up" || prop_name == "brightness_down" || prop_name.ends_with(".brightness_up") || prop_name.ends_with(".brightness_down") {
                                    (format!("\"{}\"", s), Some("keybind".to_string()))
                                } else {
                                    (format!("\"{}\"", s), None)
                                }
                            }
                            _ => (prop_val.to_string(), None),
                        };
                        if let Some(ty) = val_ty {
                            prop_parts.push(format!("{}=({}){}", prop_name, format_kdl_type(&ty), val_str));
                        } else {
                            prop_parts.push(format!("{}={}", prop_name, val_str));
                        }
                    }
                }
                if !child_parts.is_empty() {
                    let mut out = format!("{}{} {{\n", indent_str, format_kdl_identifier(key));
                    if !prop_parts.is_empty() {
                        out.push_str(&format!("{}{}\n", "    ".repeat(indent + 1), prop_parts.join(" ")));
                    }
                    for child in child_parts {
                        out.push_str(&child);
                    }
                    out.push_str(&format!("{}}}\n", indent_str));
                    out
                } else {
                    format!("{}{} {}\n", indent_str, key, prop_parts.join(" "))
                }
            }
        }
        serde_json::Value::Array(arr) => {
            let mut out = String::new();
            for item in arr {
                out.push_str(&value_to_kdl_with_annotations(key, item, indent, parent_path, annotations));
            }
            out
        }
        _ => {
            let (val_str, val_ty) = match val {
                serde_json::Value::Bool(b) => (b.to_string(), Some("bool".to_string())),
                serde_json::Value::Number(num) => {
                    if key == "light_source_position" {
                        (num.to_string(), Some("radian".to_string()))
                    } else if num.is_f64() {
                        (num.to_string(), Some("f64".to_string()))
                    } else {
                        (num.to_string(), Some("i64".to_string()))
                    }
                }
                serde_json::Value::String(s) => {
                    if let Some(anno) = annotations.get(&current_path) {
                        if anno == "vec2i" {
                            (s.clone(), Some(anno.clone()))
                        } else {
                            (format!("\"{}\"", s), Some(anno.clone()))
                        }
                    } else if s.starts_with('#') {
                        let s_clean = s.trim_start_matches('#');
                        let ty = if s_clean.len() == 8 { "rgba" } else { "rgb" };
                        (format!("\"{}\"", s), Some(ty.to_string()))
                    } else if key == "key" || key == "keybind" || key == "shortcut" || key == "open_search" || key == "close_search" || key == "delete" || key.ends_with("_key") || key.ends_with(".key") || key.ends_with(".keybind") || key.ends_with(".open_search") || key.ends_with(".close_search") || key == "brightness_up" || key == "brightness_down" || key.ends_with(".brightness_up") || key.ends_with(".brightness_down") {
                        (format!("\"{}\"", s), Some("keybind".to_string()))
                    } else {
                        (format!("\"{}\"", s), None)
                    }
                }
                _ => (val.to_string(), None),
            };
            if let Some(ty) = val_ty {
                format!("{}{} ({}){}\n", indent_str, key, format_kdl_type(&ty), val_str)
            } else {
                format!("{}{} {}\n", indent_str, key, val_str)
            }
        }
    }
}

pub fn json_to_kdl_string(val: &serde_json::Value) -> String {
    json_to_kdl_string_with_annotations(val, &std::collections::HashMap::new())
}

pub fn json_to_kdl_string_with_annotations(
    val: &serde_json::Value,
    annotations: &std::collections::HashMap<String, String>,
) -> String {
    let mut out = String::new();
    if let serde_json::Value::Object(map) = val {
        for (sec_name, sec_val) in map {
            if let serde_json::Value::Object(sec_map) = sec_val {
                out.push_str(&format!("{} {{\n", format_kdl_identifier(sec_name)));
                for (k, v) in sec_map {
                    out.push_str(&value_to_kdl_with_annotations(k, v, 1, sec_name, annotations));
                }
                out.push_str("}\n");
            } else {
                out.push_str(&value_to_kdl_with_annotations(sec_name, sec_val, 0, "", annotations));
            }
        }
    }
    out
}

pub fn get_app_recent_files_path() -> std::path::PathBuf {
    let app_name = get_app_name().unwrap_or_else(|| "cce-app".to_string());
    get_config_path().parent().unwrap().join(app_name).join("recent-files.kdl")
}

pub fn load_recent_files() -> Vec<String> {
    let path = get_app_recent_files_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(doc) = content.parse::<kdl::KdlDocument>() {
                if let Some(recent_node) = doc.get("recent") {
                    if let Some(children) = recent_node.children() {
                        let mut files = Vec::new();
                        for node in children.nodes() {
                            if node.name().value() == "file" {
                                if let Some(entry) = node.entries().first() {
                                    if let kdl::KdlValue::String(s) = entry.value() {
                                        files.push(s.clone());
                                    }
                                }
                            }
                        }
                        return files;
                    }
                }
            }
        }
    }
    Vec::new()
}

pub fn save_recent_files(files: &[String]) {
    let path = get_app_recent_files_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut kdl_str = "recent {\n".to_string();
    for file in files {
        kdl_str.push_str(&format!("    file \"{}\"\n", file));
    }
    kdl_str.push_str("}\n");
    let _ = std::fs::write(path, kdl_str);
}
