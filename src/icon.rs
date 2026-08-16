//! XDG icon-theme lookup: an `Icon=` name from a `.desktop` entry (or an SNI
//! tray item) resolved to a file on disk.
//!
//! **This is not [`crate::upload_icon`].** That one loads a *bundled* cce-icons
//! glyph by its own name (`upload_icon("folder", 32)` reads
//! `$CCE_ICONS_DIR/folder.svg`) and is how a widget draws a chevron or a copy
//! button. This module resolves a *theme* name — anything any installed app may
//! have shipped — by searching the icon-theme directories per the freedesktop
//! icon theme spec. cce's own app icons land in
//! `$XDG_DATA_HOME/icons/hicolor/scalable/apps/` (installed by `ccebuild` from
//! `cce-icons/hicolor/`), so for a cce app the two happen to resolve to the same
//! artwork by different routes.
//!
//! The search is a deliberate simplification of the spec: it walks a fixed
//! preference order of themes, sizes and extensions rather than parsing every
//! `index.theme`. That is enough for the two callers (the launcher's app list
//! and the status bar's tray) and avoids reading a ~40KB index on every lookup.

use std::path::{Path, PathBuf};

/// Themes searched, in order. `hicolor` is the spec's fallback — every
/// implementation searches it whatever the user's theme is, and it is where cce
/// installs its own app icons. `Adwaita` follows because it is present on
/// essentially every desktop and carries the generic names (`system-file-manager`,
/// `accessories-text-editor`, …) that third-party entries lean on. `$CCE_ICON_THEME`
/// prepends a preferred theme; there is no icon-theme setting in the DE, which is
/// exactly why the fallbacks have to be good.
const THEMES: [&str; 2] = ["hicolor", "Adwaita"];

/// Size directories, best first. `scalable` leads because callers rasterize it
/// to whatever size they need. The bitmap sizes that follow are ordered for the
/// 16–48px range both callers actually draw at — a list row and a status-bar
/// tray — so a modest downscale beats both upscaling a 16px icon and decoding a
/// 512px one to draw it at 18.
const SIZES: [&str; 11] = [
    "scalable", "48x48", "64x64", "32x32", "96x96", "128x128", "24x24", "22x22", "16x16",
    "256x256", "512x512",
];

/// Extensions, best first. SVG scales; PNG is what most apps actually ship.
const EXTS: [&str; 3] = ["svg", "png", "xpm"];

/// Resolve an icon name in the `apps` context — the one a `.desktop` `Icon=` key
/// lives in. See [`lookup_in`] for the general form.
pub fn lookup(name: &str) -> Option<PathBuf> {
    lookup_in(name, &["apps"])
}

/// Resolve an icon name, searching `contexts` (`"apps"`, `"status"`, …) in the
/// order given within each theme/size directory.
///
/// Per the spec an `Icon=` value may also be an **absolute path**, in which case
/// it is used directly; and it should *not* carry an extension, though entries
/// in the wild routinely include one, so a known extension is stripped before
/// searching. Returns `None` when nothing matches — every caller has a text
/// fallback, so a missing icon must not be fatal.
pub fn lookup_in(name: &str, contexts: &[&str]) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }

    // An absolute path is used as given. This is also the shape of the stale
    // entries this system replaced (Icon=/home/…/star.png), so it resolves to
    // None the moment the file goes away rather than silently searching for a
    // theme icon named after a whole path.
    if name.starts_with('/') {
        let path = Path::new(name);
        return path.is_file().then(|| path.to_path_buf());
    }

    let stem = EXTS
        .iter()
        .find_map(|e| name.strip_suffix(&format!(".{e}")))
        .unwrap_or(name);

    let themes: Vec<String> = std::env::var("CCE_ICON_THEME")
        .ok()
        .filter(|t| !t.is_empty())
        .into_iter()
        .chain(THEMES.iter().map(|t| t.to_string()))
        .collect();

    for base in base_dirs() {
        for theme in &themes {
            for size in SIZES {
                for context in contexts {
                    for ext in EXTS {
                        let path = base
                            .join(theme)
                            .join(size)
                            .join(context)
                            .join(format!("{stem}.{ext}"));
                        if path.is_file() {
                            return Some(path);
                        }
                    }
                }
            }
        }
    }

    // /usr/share/pixmaps is themeless and flat — the pre-icon-theme location,
    // still used by a long tail of packages.
    for ext in EXTS {
        let path = PathBuf::from("/usr/share/pixmaps").join(format!("{stem}.{ext}"));
        if path.is_file() {
            return Some(path);
        }
    }

    None
}

/// Resolve a theme icon name and upload it as a renderer texture, returning
/// `(image id, pixel w, pixel h)` for [`crate::scene::paint::PaintCtx::image`].
/// SVGs are rasterized at `px` on the longer side; bitmaps are uploaded at their
/// stored size and scaled by the GPU when drawn, so `px` is a hint, not the
/// result — fit the returned dimensions into the destination rect to keep the
/// aspect ratio.
///
/// **The upload is deliberately not cached, only the decode is.** An image id is
/// meaningless to any renderer other than the one that drained the upload queue
/// for it, and `cce-cloud`'s daemon builds and tears down a whole `VkRenderer`
/// per popup — a cached id from the previous popup would name GPU resources that
/// no longer exist. Caching the decoded pixels instead means a second popup
/// still skips the disk read and the rasterizer, which is where the time goes.
pub fn upload_themed(name: &str, px: u32) -> Option<(u32, u32, u32)> {
    let (pixels, w, h) = decode(name, px)?;
    Some((crate::vk::upload_rgba(pixels, w, h), w, h))
}

/// [`upload_themed`]'s cached half: name → straight RGBA8 pixels + dimensions.
fn decode(name: &str, px: u32) -> Option<(Vec<u8>, u32, u32)> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<(String, u32), Option<(Vec<u8>, u32, u32)>>>> =
        Mutex::new(None);

    let key = (name.to_string(), px);
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(hit) = cache.get(&key) {
        return hit.clone();
    }

    let loaded = (|| {
        let path = lookup(name)?;
        let data = std::fs::read(&path).ok()?;
        match path.extension().and_then(|e| e.to_str()) {
            Some("svg") => crate::rasterize_svg(&data, px),
            Some("png") => decode_png(&data),
            // XPM has no decoder here; it is in the search list because finding
            // one and skipping it still beats falling through to a worse match.
            _ => None,
        }
    })();
    cache.insert(key, loaded.clone());
    loaded
}

/// Decode a PNG to straight (un-premultiplied) RGBA8, the layout
/// [`crate::vk::upload_rgba`] takes. `EXPAND` folds palette, sub-byte grayscale
/// and `tRNS` into plain channels, which leaves only the four color types below;
/// 16-bit samples are truncated to their high byte.
fn decode_png(data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;

    let bytes = &buf[..info.buffer_size()];
    let step = if info.bit_depth == png::BitDepth::Sixteen { 2 } else { 1 };
    let samples: Vec<u8> = bytes.iter().step_by(step).copied().collect();

    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        // EXPAND has already turned an indexed image into one of the above.
        png::ColorType::Indexed => return None,
    };

    let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
    for px in samples.chunks_exact(channels) {
        let (r, g, b, a) = match channels {
            4 => (px[0], px[1], px[2], px[3]),
            3 => (px[0], px[1], px[2], 255),
            2 => (px[0], px[0], px[0], px[1]),
            _ => (px[0], px[0], px[0], 255),
        };
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    if rgba.len() != (info.width * info.height * 4) as usize {
        return None;
    }
    Some((rgba, info.width, info.height))
}

/// Icon base directories in spec precedence: `$XDG_DATA_HOME/icons`, then the
/// legacy `~/.icons`, then `icons` under each `$XDG_DATA_DIRS` entry.
fn base_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);

    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(v) => dirs.push(PathBuf::from(v).join("icons")),
        None => {
            if let Some(h) = &home {
                dirs.push(h.join(".local/share/icons"));
            }
        }
    }
    if let Some(h) = &home {
        dirs.push(h.join(".icons"));
    }

    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for dir in std::env::split_paths(&data_dirs) {
        if !dir.as_os_str().is_empty() {
            dirs.push(dir.join("icons"));
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_name_resolves_to_nothing() {
        assert!(lookup("").is_none());
    }

    #[test]
    fn absolute_path_is_used_as_given() {
        let dir = std::env::temp_dir().join("cce-ui-icon-abs");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("thing.png");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(lookup(file.to_str().unwrap()), Some(file.clone()));

        // A dead absolute path must not fall through to a theme search.
        std::fs::remove_file(&file).unwrap();
        assert!(lookup(file.to_str().unwrap()).is_none());
    }

    #[test]
    fn finds_a_scalable_apps_icon_and_strips_a_given_extension() {
        let root = std::env::temp_dir().join("cce-ui-icon-theme");
        let apps = root.join("icons/hicolor/scalable/apps");
        std::fs::create_dir_all(&apps).unwrap();
        let icon = apps.join("cce-widget.svg");
        std::fs::write(&icon, b"<svg/>").unwrap();

        // Scoped env mutation: these tests share a process, so keep it to one test.
        let prev = std::env::var_os("XDG_DATA_HOME");
        unsafe { std::env::set_var("XDG_DATA_HOME", &root) };

        assert_eq!(lookup("cce-widget"), Some(icon.clone()));
        // Entries in the wild include the extension even though the spec says not to.
        assert_eq!(lookup("cce-widget.svg"), Some(icon));
        // The apps context must not match a name that is not there.
        assert!(lookup("cce-absent").is_none());
        // A different context must not see it.
        assert!(lookup_in("cce-widget", &["status"]).is_none());

        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_DATA_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_DATA_HOME") },
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
