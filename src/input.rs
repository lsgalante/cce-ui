// ~/.config/cce/input.kdl — domain-scoped keybindings for the whole desktop.
//
// Top-level nodes are DOMAINS; their children are bindings:
//
//     cce-window-manager {
//         close_window "super+q"
//         spawn "super+d" command="cce-cloud --apps"
//     }
//     cce-ui {
//         open_search "ctrl+f"      // toolkit-wide widget defaults
//     }
//     cce-files {
//         open_search "/"           // per-app override of the cce-ui default
//     }
//
// The chord is the first string argument (a `(keybind)` type annotation is
// accepted and ignored); a `key="..."` property works too. Extra properties
// (e.g. `command=` for spawn) ride along on the entry.
//
// Resolution order for an app is `<app>.<name>` → `cce-ui.<name>`; widgets
// match the resolved chord string with `widget::match_key_shortcut`. The
// `cce-window-manager` domain is consumed by the compositor, which maps
// names to policy `Action`s — chords never get interpreted here.

use std::collections::BTreeMap;

/// Domain holding toolkit-wide default widget bindings.
pub const UI_DOMAIN: &str = "cce-ui";
/// Domain holding compositor / window-management bindings.
pub const WINDOW_MANAGER_DOMAIN: &str = "cce-window-manager";

/// `~/.config/cce/input.kdl` (honoring `XDG_CONFIG_HOME`).
pub fn get_input_path() -> std::path::PathBuf {
    crate::config::cce_config_dir().join("input.kdl")
}

/// One binding line inside a domain block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingEntry {
    /// Node name, e.g. `open_search`. Names may repeat (several `spawn`s).
    pub name: String,
    /// The chord string, e.g. `"super+shift+h"`.
    pub chord: String,
    /// `command="..."` property, for entries that launch something.
    pub command: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InputConfig {
    domains: BTreeMap<String, Vec<BindingEntry>>,
}

impl InputConfig {
    /// Parse the file content. Domain blocks with no children are ignored;
    /// a child with no chord (no string argument and no `key=`) is skipped.
    pub fn parse(content: &str) -> Result<InputConfig, String> {
        let doc: kdl::KdlDocument = content.parse().map_err(|e| format!("{}", e))?;
        let mut domains: BTreeMap<String, Vec<BindingEntry>> = BTreeMap::new();
        for domain_node in doc.nodes() {
            let Some(children) = domain_node.children() else { continue };
            let entries = domains.entry(domain_node.name().value().to_string()).or_default();
            for node in children.nodes() {
                let mut chord: Option<String> = None;
                let mut command: Option<String> = None;
                for entry in node.entries() {
                    let value = match entry.value() {
                        kdl::KdlValue::String(s) | kdl::KdlValue::RawString(s) => s.clone(),
                        _ => continue,
                    };
                    match entry.name().map(|n| n.value()) {
                        None | Some("key") => {
                            if chord.is_none() {
                                chord = Some(value);
                            }
                        }
                        Some("command") => command = Some(value),
                        Some(_) => {}
                    }
                }
                if let Some(chord) = chord {
                    entries.push(BindingEntry {
                        name: node.name().value().to_string(),
                        chord,
                        command,
                    });
                }
            }
        }
        Ok(InputConfig { domains })
    }

    /// Load `input.kdl`. Missing file → empty config; a parse error is
    /// logged and also yields an empty config, so callers fall back to
    /// their defaults instead of losing all input.
    pub fn load() -> InputConfig {
        let path = get_input_path();
        let Ok(content) = std::fs::read_to_string(&path) else {
            return InputConfig::default();
        };
        match InputConfig::parse(&content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("[cce-ui] failed to parse {}: {}", path.display(), e);
                InputConfig::default()
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.domains.values().all(|v| v.is_empty())
    }

    /// All entries of one domain, in file order.
    pub fn domain(&self, domain: &str) -> &[BindingEntry] {
        self.domains.get(domain).map(Vec::as_slice).unwrap_or(&[])
    }

    /// First entry named `name` in `domain`, no fallback.
    pub fn get(&self, domain: &str, name: &str) -> Option<&BindingEntry> {
        self.domain(domain).iter().find(|e| e.name == name)
    }

    /// Domain resolution for apps: `<app>.<name>`, falling back to
    /// `cce-ui.<name>`.
    pub fn resolve(&self, app: &str, name: &str) -> Option<&BindingEntry> {
        self.get(app, name).or_else(|| self.get(UI_DOMAIN, name))
    }

    /// Resolved chord string for widgets, with a compiled-in default as the
    /// last resort.
    pub fn resolve_chord(&self, app: &str, name: &str, default: &str) -> String {
        self.resolve(app, name).map(|e| e.chord.clone()).unwrap_or_else(|| default.to_string())
    }
}

/// Replace (or append) one domain block in `input.kdl` content, leaving all
/// other domains and their formatting untouched. `entries` becomes the whole
/// new block, in order; an empty slice removes the domain. Pure — the I/O
/// wrapper is `write_domain`.
pub fn upsert_domain(content: &str, domain: &str, entries: &[BindingEntry]) -> Result<String, String> {
    let mut doc: kdl::KdlDocument = if content.trim().is_empty() {
        kdl::KdlDocument::new()
    } else {
        content.parse().map_err(|e| format!("{}", e))?
    };

    let mut block = format!("{} {{\n", kdl_ident(domain));
    for entry in entries {
        block.push_str(&format!("    {} (keybind){:?}", kdl_ident(&entry.name), entry.chord));
        if let Some(ref command) = entry.command {
            block.push_str(&format!(" command={:?}", command));
        }
        block.push('\n');
    }
    block.push_str("}\n");

    doc.nodes_mut().retain(|n| n.name().value() != domain);
    if !entries.is_empty() {
        let node: kdl::KdlNode = block.parse().map_err(|e| format!("{}", e))?;
        doc.nodes_mut().push(node);
    }
    let mut out = doc.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// Quote a node name if it isn't a bare KDL identifier.
fn kdl_ident(name: &str) -> String {
    let bare = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit());
    if bare { name.to_string() } else { format!("{:?}", name) }
}

/// Rewrite one domain of the file at `path` (created if missing). This is
/// the editor API for settings UIs and migration tools.
pub fn write_domain(path: &std::path::Path, domain: &str, entries: &[BindingEntry]) -> Result<(), String> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let updated = upsert_domain(&content, domain, entries)?;
    let path_str = path.to_string_lossy();
    if crate::config::safe_write(&path_str, &updated) {
        Ok(())
    } else {
        Err(format!("failed to write {}", path_str))
    }
}

static CACHED: std::sync::OnceLock<InputConfig> = std::sync::OnceLock::new();

/// Process-wide cached `input.kdl`, loaded on first use. Widget-default
/// getters go through this so the file is read once per app.
pub fn cached() -> &'static InputConfig {
    CACHED.get_or_init(InputConfig::load)
}

/// Chord for a widget binding, resolved by specificity: the app's own
/// `input.kdl` domain, then the caller-supplied legacy value (per-widget
/// `config.kdl` props), then the toolkit-wide `cce-ui` domain, then the
/// compiled-in default.
pub fn widget_chord(name: &str, legacy: &str, default: &str) -> String {
    let input = cached();
    if let Some(app) = crate::config::get_app_name() {
        if let Some(e) = input.get(&app, name) {
            return e.chord.clone();
        }
    }
    if !legacy.is_empty() {
        return legacy.to_string();
    }
    if let Some(e) = input.get(UI_DOMAIN, name) {
        return e.chord.clone();
    }
    default.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
cce-window-manager {
    close_window "super+q"
    toggle_fullscreen (keybind)"super+f"
    spawn "super+d" command="cce-cloud --apps"
    spawn "super+t" command="foot"
}
cce-ui {
    open_search "ctrl+f"
    close_search "escape"
}
cce-files {
    open_file key="enter"
    open_search "/"
}
"#;

    #[test]
    fn parses_domains_and_entries() {
        let c = InputConfig::parse(SAMPLE).unwrap();
        assert_eq!(c.domain(WINDOW_MANAGER_DOMAIN).len(), 4);
        assert_eq!(c.get(WINDOW_MANAGER_DOMAIN, "close_window").unwrap().chord, "super+q");
        // Type annotations are transparent.
        assert_eq!(c.get(WINDOW_MANAGER_DOMAIN, "toggle_fullscreen").unwrap().chord, "super+f");
        // Repeated names keep every entry, in order, with their commands.
        let spawns: Vec<_> =
            c.domain(WINDOW_MANAGER_DOMAIN).iter().filter(|e| e.name == "spawn").collect();
        assert_eq!(spawns.len(), 2);
        assert_eq!(spawns[0].command.as_deref(), Some("cce-cloud --apps"));
        assert_eq!(spawns[1].chord, "super+t");
        // key= property form.
        assert_eq!(c.get("cce-files", "open_file").unwrap().chord, "enter");
    }

    #[test]
    fn resolution_prefers_app_over_ui_domain() {
        let c = InputConfig::parse(SAMPLE).unwrap();
        // Overridden in cce-files.
        assert_eq!(c.resolve("cce-files", "open_search").unwrap().chord, "/");
        // Not overridden: falls back to cce-ui.
        assert_eq!(c.resolve("cce-files", "close_search").unwrap().chord, "escape");
        // Unknown app: pure cce-ui fallback.
        assert_eq!(c.resolve("cce-email", "open_search").unwrap().chord, "ctrl+f");
        // Nowhere: compiled-in default.
        assert_eq!(c.resolve_chord("cce-email", "save", "ctrl+s"), "ctrl+s");
    }

    #[test]
    fn upsert_domain_round_trips() {
        let entries = vec![
            BindingEntry { name: "close_window".into(), chord: "super+q".into(), command: None },
            BindingEntry {
                name: "spawn".into(),
                chord: "super+d".into(),
                command: Some("cce-cloud --apps".into()),
            },
        ];
        // Insert into empty content, then read back.
        let out = upsert_domain("", WINDOW_MANAGER_DOMAIN, &entries).unwrap();
        let parsed = InputConfig::parse(&out).unwrap();
        assert_eq!(parsed.domain(WINDOW_MANAGER_DOMAIN).to_vec(), entries);

        // Replace the domain without touching other domains.
        let combined = format!("{}\n{}", SAMPLE, ""); // SAMPLE already has the domain
        let replaced = upsert_domain(
            &combined,
            WINDOW_MANAGER_DOMAIN,
            &entries[..1],
        )
        .unwrap();
        let parsed = InputConfig::parse(&replaced).unwrap();
        assert_eq!(parsed.domain(WINDOW_MANAGER_DOMAIN).len(), 1);
        assert_eq!(parsed.get("cce-files", "open_search").unwrap().chord, "/");

        // Empty entries removes the block entirely.
        let removed = upsert_domain(&replaced, WINDOW_MANAGER_DOMAIN, &[]).unwrap();
        let parsed = InputConfig::parse(&removed).unwrap();
        assert!(parsed.domain(WINDOW_MANAGER_DOMAIN).is_empty());
        assert_eq!(parsed.resolve("cce-files", "close_search").unwrap().chord, "escape");
    }

    #[test]
    fn empty_and_invalid_input() {
        assert!(InputConfig::parse("").unwrap().is_empty());
        // Chord-less entries are skipped, childless nodes ignored.
        let c = InputConfig::parse("cce-ui {\n    broken\n}\nstray-node\n").unwrap();
        assert!(c.is_empty());
        assert!(InputConfig::parse("cce-ui {").is_err());
    }
}
