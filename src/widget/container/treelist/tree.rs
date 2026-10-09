//! The tree itself: parsing key paths, matching a search, building the rows from the value, and
//! the operations on it — copying a key or value, deleting a key, expanding and collapsing.

use super::*;

#[derive(Debug, Clone)]
pub(super) enum PathToken {
    Key(String),
    Index(usize),
}

pub(super) fn parse_path(path: &str) -> Vec<PathToken> {
    let mut tokens = Vec::new();
    for part in path.split('.') {
        if part.is_empty() { continue; }
        if let Some(bracket_idx) = part.find('[') {
            let name = &part[..bracket_idx];
            if !name.is_empty() {
                tokens.push(PathToken::Key(name.to_string()));
            }
            let mut rest = &part[bracket_idx..];
            while let Some(start) = rest.find('[') {
                if let Some(end) = rest.find(']') {
                    let idx_str = &rest[start + 1..end];
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        tokens.push(PathToken::Index(idx));
                    }
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
            }
        } else {
            tokens.push(PathToken::Key(part.to_string()));
        }
    }
    tokens
}

pub(super) fn matches_query(key_path: &str, val: &serde_json::Value, annotation: Option<&str>, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query_lower = query.to_lowercase();
    if key_path.to_lowercase().contains(&query_lower) {
        return true;
    }
    let val_str = match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    if val_str.to_lowercase().contains(&query_lower) {
        return true;
    }
    if let Some(anno) = annotation {
        if anno.to_lowercase().contains(&query_lower) {
            return true;
        }
    }
    false
}

pub(super) fn build_tree(
    flat_keys: &[(String, serde_json::Value)],
    annotations: &[Option<String>],
    collapsed_sections: &HashSet<String>,
    query: &str,
) -> Vec<TreeElement> {
    let mut items = Vec::new();
    let mut seen_prefixes = HashSet::new();

    let mut matching_indices = HashSet::new();
    for (idx, (key_path, val)) in flat_keys.iter().enumerate() {
        let annotation = annotations.get(idx).and_then(|opt| opt.as_deref());
        if query.is_empty() || matches_query(key_path, val, annotation, query) {
            matching_indices.insert(idx);
        }
    }

    for (original_idx, (key_path, val)) in flat_keys.iter().enumerate() {
        if !matching_indices.contains(&original_idx) {
            continue;
        }
        let tokens = parse_path(key_path);
        let mut current_prefix = String::new();
        let mut is_hidden = false;
        
        for i in 0..tokens.len() {
            let token = &tokens[i];
            let part_name = match token {
                PathToken::Key(k) => {
                    if current_prefix.is_empty() {
                        current_prefix = k.clone();
                    } else {
                        current_prefix = format!("{}.{}", current_prefix, k);
                    }
                    k.clone()
                }
                PathToken::Index(idx) => {
                    let s = format!("[{}]", idx);
                    current_prefix = format!("{}{}", current_prefix, s);
                    s
                }
            };

            let is_last = i == tokens.len() - 1;
            
            if is_hidden {
                continue;
            }

            if is_last {
                items.push(TreeElement::Leaf {
                    path: key_path.clone(),
                    name: part_name,
                    indent: i,
                    val: val.clone(),
                    original_idx,
                });
            } else {
                if !seen_prefixes.contains(&current_prefix) {
                    seen_prefixes.insert(current_prefix.clone());
                    let collapsed = collapsed_sections.contains(&current_prefix);
                    items.push(TreeElement::Section {
                        path: current_prefix.clone(),
                        name: part_name,
                        indent: i,
                        collapsed,
                    });
                }
                if collapsed_sections.contains(&current_prefix) {
                    is_hidden = true;
                }
            }
        }
    }
    items
}

impl TreeList {
    // The tree context-menu actions, dispatched by the global context menu through
    // `Input::context_action`.
    pub(super) fn copy_key(&self) {
        if let Some(idx) = self.selected_key_idx {
            if idx < self.flat_keys.len() {
                let key_path = &self.flat_keys[idx].0;
                clipboard::copy_to_clipboard(key_path);
            }
        }
    }

    pub(super) fn copy_value(&self) {
        if let Some(idx) = self.selected_key_idx {
            if idx < self.flat_keys.len() {
                let val = &self.flat_keys[idx].1;
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Number(n) => n.to_string(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                };
                clipboard::copy_to_clipboard(&val_str);
            }
        }
    }

    pub(super) fn delete_key(&mut self) {
        if let Some(idx) = self.selected_key_idx {
            if idx < self.flat_keys.len() {
                let key_path = self.flat_keys[idx].0.clone();
                self.deleted_key_path = Some(key_path);
            }
        }
    }

    pub(super) fn expand_node(&mut self) {
        if let Some(path) = self.right_clicked_section.clone() {
            self.collapsed_sections.remove(&path);
            self.rebuild_tree();
            self.clicked_item = Some(TreeElement::Section {
                path,
                name: String::new(),
                indent: 0,
                collapsed: false,
            });
        }
        self.right_clicked_section = None;
    }

    pub(super) fn collapse_node(&mut self) {
        if let Some(path) = self.right_clicked_section.clone() {
            self.collapsed_sections.insert(path.clone());
            self.rebuild_tree();
            self.clicked_item = Some(TreeElement::Section {
                path,
                name: String::new(),
                indent: 0,
                collapsed: true,
            });
        }
        self.right_clicked_section = None;
    }

    pub(super) fn expand_all_nodes(&mut self) {
        self.collapsed_sections.clear();
        self.rebuild_tree();
        if let Some(path) = self.right_clicked_section.clone() {
            self.clicked_item = Some(TreeElement::Section {
                path,
                name: String::new(),
                indent: 0,
                collapsed: false,
            });
        } else {
            self.clicked_item = Some(TreeElement::Section {
                path: String::new(),
                name: String::new(),
                indent: 0,
                collapsed: false,
            });
        }
        self.right_clicked_section = None;
    }

    pub(super) fn collapse_all_nodes(&mut self) {
        self.collapsed_sections = self.get_all_section_paths();
        self.rebuild_tree();
        if let Some(path) = self.right_clicked_section.clone() {
            self.clicked_item = Some(TreeElement::Section {
                path,
                name: String::new(),
                indent: 0,
                collapsed: true,
            });
        } else {
            self.clicked_item = Some(TreeElement::Section {
                path: String::new(),
                name: String::new(),
                indent: 0,
                collapsed: true,
            });
        }
        self.right_clicked_section = None;
    }

}

impl TreeList {
    pub(super) fn get_all_section_paths(&self) -> HashSet<String> {
        let mut sections = HashSet::new();
        for (key_path, _) in &self.flat_keys {
            let tokens = parse_path(key_path);
            let mut current_prefix = String::new();
            for i in 0..(tokens.len().saturating_sub(1)) {
                let token = &tokens[i];
                match token {
                    PathToken::Key(k) => {
                        if current_prefix.is_empty() {
                            current_prefix = k.clone();
                        } else {
                            current_prefix = format!("{}.{}", current_prefix, k);
                        }
                    }
                    PathToken::Index(idx) => {
                        let s = format!("[{}]", idx);
                        current_prefix = format!("{}{}", current_prefix, s);
                    }
                };
                sections.insert(current_prefix.clone());
            }
        }
        sections
    }
}
