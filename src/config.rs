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
                        if let Some(num) = serde_json::Number::from_f64(*f) {
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
                                    if let Some(num) = serde_json::Number::from_f64(*f) {
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
        } else if let Some(children) = node.children() {
            kdl_to_json(children)
        } else if has_props {
            serde_json::Value::Object(node_map)
        } else if let Some(entry) = node.entries().first() {
            match entry.value() {
                kdl::KdlValue::Bool(b) => serde_json::Value::Bool(*b),
                kdl::KdlValue::Base2(i) |
                kdl::KdlValue::Base8(i) |
                kdl::KdlValue::Base10(i) |
                kdl::KdlValue::Base16(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
                kdl::KdlValue::Base10Float(f) => {
                    if let Some(num) = serde_json::Number::from_f64(*f) {
                        serde_json::Value::Number(num)
                    } else {
                        serde_json::Value::Null
                    }
                }
                kdl::KdlValue::String(s) |
                kdl::KdlValue::RawString(s) => serde_json::Value::String(s.clone()),
                kdl::KdlValue::Null => serde_json::Value::Null,
            }
        } else {
            serde_json::Value::Null
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

pub fn parse_kdl_to_json(content: &str) -> serde_json::Value {
    if let Ok(doc) = content.parse::<kdl::KdlDocument>() {
        kdl_to_json(&doc)
    } else {
        serde_json::json!({})
    }
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
    "status", "overlay", "backplate", "desktop", "list", "section", "textbox"
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

pub fn update_kdl_in_memory(doc: &mut kdl::KdlDocument, key: &str, value: &str, _default_section: &str) -> bool {
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
        if ext_ty.starts_with("menu:") {
            kdl_ty = Some(ext_ty.clone());
        }
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
        let mut entry = kdl::KdlEntry::new(kdl_val);
        if let Some(ref ty) = kdl_ty {
            entry.set_ty(ty.as_str());
        }
        child_node.entries_mut().push(entry);
    }

    true
}

pub fn get_config_path() -> std::path::PathBuf {
    let dir = if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg_config.is_empty() {
            std::path::PathBuf::from(xdg_config)
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/home/lsgalante".to_string());
            std::path::PathBuf::from(home).join(".config")
        }
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/lsgalante".to_string());
        std::path::PathBuf::from(home).join(".config")
    };
    dir.join("cce").join("config.kdl")
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

fn safe_write(path: &str, content: &str) -> bool {
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
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut doc = match content.parse::<kdl::KdlDocument>() {
        Ok(d) => d,
        Err(_) => kdl::KdlDocument::new(),
    };
    
    if update_kdl_in_memory(&mut doc, key, value, default_section) {
        let updated_str = doc.to_string();
        return safe_write(path, &updated_str);
    }
    false
}

pub fn write_keybindings_to_kdl(path: &str, keybinds: &[serde_json::Value]) -> bool {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut doc = match content.parse::<kdl::KdlDocument>() {
        Ok(d) => d,
        Err(_) => kdl::KdlDocument::new(),
    };

    // Remove all existing key_bindings nodes (root-level)
    doc.nodes_mut().retain(|n| n.name().value() != "key_bindings");

    // Construct nested key_bindings block
    let mut block_str = "key_bindings {\n".to_string();
    for v in keybinds {
        if let Some(obj) = v.as_object() {
            let mods = obj.get("mods").and_then(|m| m.as_str()).unwrap_or("");
            let key = obj.get("key").and_then(|k| k.as_str()).unwrap_or("");
            let action = obj.get("action").and_then(|a| a.as_str()).unwrap_or("");
            let command = obj.get("command").and_then(|c| c.as_str()).unwrap_or("");

            block_str.push_str("    bind");
            if !action.is_empty() {
                block_str.push_str(&format!(" action={:?}", action));
            }
            if !command.is_empty() {
                block_str.push_str(&format!(" command={:?}", command));
            }
            if !key.is_empty() {
                block_str.push_str(&format!(" key={:?}", key));
            }
            if !mods.is_empty() {
                block_str.push_str(&format!(" mods={:?}", mods));
            }
            block_str.push('\n');
        }
    }
    block_str.push_str("}\n");

    if let Ok(node) = block_str.parse::<kdl::KdlNode>() {
        if let Some(input_idx) = doc.nodes().iter().position(|n| n.name().value() == "input") {
            let input_node = &mut doc.nodes_mut()[input_idx];
            let children = input_node.ensure_children();
            children.nodes_mut().retain(|n| n.name().value() != "key_bindings");
            children.nodes_mut().push(node);
        } else {
            doc.nodes_mut().push(node);
        }
    }

    let updated_str = doc.to_string();
    safe_write(path, &updated_str)
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
    fn test_get_kdl_type_annotation() {
        let content = "input {\n    accel_profile (\"menu:flat,adaptive,none,custom\")\"flat\"\n    touchpad {\n        gestures pinch=(bool)true\n    }\n}\n";
        let ty1 = get_kdl_type_annotation(content, "input.accel_profile");
        assert_eq!(ty1, Some("menu:flat,adaptive,none,custom".to_string()));
        
        let ty2 = get_kdl_type_annotation(content, "input.touchpad.gestures.pinch");
        assert_eq!(ty2, Some("bool".to_string()));
    }
}
