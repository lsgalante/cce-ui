//! `ColorSelector`: a colour as an editable hex field with a swatch beside it. Typing edits the
//! hex in place; a press on the swatch launches the DE's picker (`command`, `cce-color-editor` by
//! default) with `--stream`, placed at the pointer through the compositor, and every colour the
//! picker prints applies while it stays open — a `cancel` restores the value it was opened on.
//! With `with_alpha` the hex carries an alpha byte and the swatch shows it over a checker. The
//! recessed style (the default under relief) cuts the field and swatch as one well.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, construction and builders, the field's relief, the hex value, `impl Layout`, placing the picker |
//! | `paint` | `impl Paint`: the field, the hex text and caret, the swatch and its checker |
//! | `input` | `impl Input`: editing the hex, launching the picker and reading its stream in the tick, the reader's text |

mod input;
mod paint;
#[cfg(test)]
mod tests;

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::*;

#[derive(Debug)]
pub struct ColorSelector {
    pub color: [u8; 3],
    pub alpha: u8,
    just_clicked: bool,
    pub editing: bool,
    pub(crate) edit_buffer: String,
    pub cursor_idx: usize,
    pub font_family: String,
    pub command: String,
    hovered: bool,
    child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    pub editor_state: TextEditorState,
    pub just_changed: bool,
    pub with_alpha: bool,
    /// Live color lines from the running picker (`cce-color-editor --stream` prints
    /// every change), forwarded by a reader thread — the value applies while
    /// the editor stays open instead of on exit.
    live_rx: Option<std::sync::mpsc::Receiver<String>>,
    /// The value at picker launch, restored when the stream reports `cancel`.
    revert_hex: Option<String>,
    /// Char-index → x offsets of the drawn hex text, recorded by
    /// [`Paint::prepare_text`] from the same shaped buffer `ctx.text` draws
    /// (size 12, default family). The caret reads these; the SVG-rasterized
    /// `measure_text` prefix it used before reports inked extent, which drifts
    /// off the glyph advances. Empty until the first shape.
    glyph_offsets: Vec<f32>,
    /// Recessed style: the hex field is a well carved into the plate below
    /// (the TextBox's, rim lit while editing) and the swatch a raised bevel
    /// plate of its colour, instead of the hairline frame and the flat swatch
    /// with its glow. Defaults to `control_relief()`.
    recessed: Option<bool>,
}

impl Clone for ColorSelector {
    fn clone(&self) -> Self {
        Self {
            color: self.color,
            alpha: self.alpha,
            just_clicked: self.just_clicked,
            editing: self.editing,
            edit_buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            font_family: self.font_family.clone(),
            command: self.command.clone(),
            hovered: self.hovered,
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: self.editor_state.clone(),
            just_changed: self.just_changed,
            with_alpha: self.with_alpha,
            live_rx: None,
            revert_hex: None,
            glyph_offsets: self.glyph_offsets.clone(),
            recessed: self.recessed,
        }
    }
}

impl ColorSelector {
    /// The style in force: the per-widget override (`with_recessed`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn recessed(&self) -> bool {
        self.recessed.unwrap_or_else(crate::layout::control_relief)
    }

    pub fn new(color: [u8; 3]) -> Adapted<ColorSelector> {
        Adapted::new(ColorSelector {
            color,
            alpha: 255,
            just_clicked: false,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            font_family: crate::layout::color_selector_font(),
            command: "cce-color-editor".to_string(),
            hovered: false,
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            with_alpha: false,
            live_rx: None,
            revert_hex: None,
            glyph_offsets: Vec::new(),
            recessed: None,
        })
    }

    pub fn new_rgba(color: [u8; 4]) -> Adapted<ColorSelector> {
        Adapted::new(ColorSelector {
            color: [color[0], color[1], color[2]],
            alpha: color[3],
            just_clicked: false,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            font_family: crate::layout::color_selector_font(),
            command: "cce-color-editor".to_string(),
            hovered: false,
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            with_alpha: true,
            live_rx: None,
            revert_hex: None,
            glyph_offsets: Vec::new(),
            recessed: None,
        })
    }

    /// The hex field's well for hosts that draw this control through the legacy
    /// flat views (see `ParametersBg::reliefs`): (x, y, w, h, radius, depth) over the
    /// widget's assigned content `rect`, or None when the style is off. The same
    /// geometry `paint` carves (untinted).
    pub fn field_relief(&self, rect: Rect) -> Option<(f32, f32, f32, f32, f32, f32)> {
        if !self.recessed() {
            return None;
        }
        let well_h = crate::layout::color_selector_height().min(rect.height);
        let radius = crate::layout::textbox_corner_radius();
        let depth = crate::layout::bevel_width().min(well_h * 0.2);
        // One well across the whole control: the hex text and the swatch are
        // segments of its floor (the Breadcrumb composition), not two parts.
        let (well, radii) = crate::layout::carve_inside(
            Rect { x: rect.x, y: rect.y, width: rect.width, height: well_h },
            (radius, radius, radius, radius),
            depth,
        );
        Some((well.x, well.y, well.width, well.height, radii.0, depth))
    }

}

impl Adapted<ColorSelector> {
    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = Some(recessed);
        self
    }

    pub fn with_alpha(mut self, with_alpha: bool) -> Self {
        self.with_alpha = with_alpha;
        self
    }

    pub fn with_font_family(mut self, font_family: &str) -> Self {
        self.font_family = font_family.to_string();
        self
    }

    pub fn with_command(mut self, command: &str) -> Self {
        self.command = command.to_string();
        self
    }
}

impl ColorSelector {
    fn value_hex(&self) -> String {
        if self.with_alpha {
            Some(format!("#{:02x}{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2], self.alpha))
        } else {
            Some(format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]))
        }
    
        .unwrap()
    }

    fn begin_edit(&mut self) {
        self.editing = true;
        self.edit_buffer = self.value_hex();
        self.cursor_idx = self.edit_buffer.chars().count();
    }
}

impl Layout for ColorSelector {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::color_selector_height()))
    }
}

fn parse_hex(s: &str) -> Option<[u8; 4]> {
    crate::color::parse_hex_bytes(s)
}

/// Ask the compositor to open the picker at this control instead of its
/// remembered position. Native only: in a browser there is no compositor to ask.
#[cfg(not(target_arch = "wasm32"))]
fn place_picker_at_pointer(command: &str) {
    // The pointer is on the swatch right now, so its location IS the
    // control's location. One-shot, best-effort (`place-next` consumed at
    // the picker's map; ignored off-cce).
    if let Ok(reply) = crate::ipc::send_command("cce", "pointer-location") {
        let mut px = None;
        let mut py = None;
        for tok in reply.split_whitespace() {
            if let Some(v) = tok.strip_prefix("x=") {
                px = v.parse::<f64>().ok();
            } else if let Some(v) = tok.strip_prefix("y=") {
                py = v.parse::<f64>().ok();
            }
        }
        if let (Some(x), Some(y)) = (px, py) {
            let app_id = std::path::Path::new(&command)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| command.to_string());
            let _ = crate::ipc::send_command(
                "cce",
                &format!("place-next {} {:.0} {:.0}", app_id, x, y),
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn place_picker_at_pointer(_command: &str) {}
