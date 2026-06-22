use std::fs;
use serde_json::Value;

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

pub fn write_config_value(path: &str, key: &str, value: &str, default_section: &str) -> bool {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut val: Value = serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}));
    
    if update_json_in_memory(&mut val, key, value, default_section) {
        if let Ok(updated_str) = serde_json::to_string_pretty(&val) {
            return fs::write(path, updated_str).is_ok();
        }
    }
    false
}
