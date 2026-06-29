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

        let val = if let Some(children) = node.children() {
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
                    arr.push(val);
                    map.insert(name, serde_json::Value::Array(arr));
                }
                other => {
                    map.insert(name, serde_json::Value::Array(vec![other, val]));
                }
            }
        } else {
            let list_names = ["keybind", "pointer_bind", "gesture_bind", "mode_rule", "tag_layout", "startup", "device"];
            if list_names.contains(&name.as_str()) {
                map.insert(name, serde_json::Value::Array(vec![val]));
            } else {
                map.insert(name, val);
            }
        }
    }
    serde_json::Value::Object(map)
}

pub fn parse_kdl_to_json(content: &str) -> serde_json::Value {
    if let Ok(doc) = content.parse::<kdl::KdlDocument>() {
        let val = kdl_to_json(&doc);
        if let Some(obj) = val.as_object() {
            if obj.is_empty() && (content.trim().starts_with('{') || content.trim().starts_with('[')) {
                if let Ok(j) = serde_json::from_str::<serde_json::Value>(content) {
                    return j;
                }
            }
        }
        val
    } else {
        if let Ok(j) = serde_json::from_str::<serde_json::Value>(content) {
            j
        } else {
            serde_json::json!({})
        }
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

pub fn update_kdl_in_memory(doc: &mut kdl::KdlDocument, key: &str, value: &str, default_section: &str) -> bool {
    let (target_section, target_node, target_prop) = parse_config_path(key, default_section);
    
    let section_node = if let Some(node) = doc.nodes_mut().iter_mut().find(|n| n.name().value() == target_section) {
        node
    } else {
        if let Ok(new_node) = format!("{}\n", target_section).parse::<kdl::KdlNode>() {
            doc.nodes_mut().push(new_node);
            doc.nodes_mut().last_mut().unwrap()
        } else {
            return false;
        }
    };

    let children = section_node.ensure_children();

    let child_node = if let Some(child) = children.nodes_mut().iter_mut().find(|n| n.name().value() == target_node) {
        child
    } else {
        if let Ok(new_child) = format!("{}\n", target_node).parse::<kdl::KdlNode>() {
            children.nodes_mut().push(new_child);
            children.nodes_mut().last_mut().unwrap()
        } else {
            return false;
        }
    };

    let (kdl_val, kdl_ty) = if let Ok(b) = value.parse::<bool>() {
        (kdl::KdlValue::Bool(b), Some("bool"))
    } else if value.starts_with('#') {
        (kdl::KdlValue::String(value.to_string()), Some("color"))
    } else if value.contains('.') {
        if let Ok(f) = value.parse::<f64>() {
            (kdl::KdlValue::Base10Float(f), Some("f64"))
        } else {
            (kdl::KdlValue::String(value.to_string()), None)
        }
    } else if let Ok(i) = value.parse::<i64>() {
        (kdl::KdlValue::Base10(i), Some("i64"))
    } else {
        let s = value.trim_matches('"').to_string();
        (kdl::KdlValue::String(s), None)
    };

    if let Some(prop_name) = target_prop {
        let mut found = false;
        for entry in child_node.entries_mut() {
            if let Some(id) = entry.name() {
                if id.value() == prop_name {
                    *entry = kdl::KdlEntry::new_prop(prop_name.clone(), kdl_val.clone());
                    if let Some(ty) = kdl_ty {
                        entry.set_ty(ty);
                    }
                    found = true;
                    break;
                }
            }
        }
        if !found {
            let mut entry = kdl::KdlEntry::new_prop(prop_name, kdl_val);
            if let Some(ty) = kdl_ty {
                entry.set_ty(ty);
            }
            child_node.entries_mut().push(entry);
        }
    } else {
        child_node.entries_mut().clear();
        let mut entry = kdl::KdlEntry::new(kdl_val);
        if let Some(ty) = kdl_ty {
            entry.set_ty(ty);
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
}
