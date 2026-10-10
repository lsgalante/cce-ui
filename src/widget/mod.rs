//! The widget toolkit: the widgets (`input`, `container`, `display`, `core`), the narrow traits
//! and their adapter (`model`), the registry's handles and embedded children, and the shared
//! scroll, swipe, line-edit and shaping models. Everything an app names is re-exported here.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | widget ids, layout value types, `ContextAction`, `EmbedImage`, `CornerRadii`, the module tree and its re-exports |
//! | `events` | the input vocabulary (`Event`, keys, buttons, wheel deltas) and shortcut matching |
//! | `host` | `WidgetHost`, the surface the machinery sees, and `WidgetHostExt`'s derived reads |
//! | `controllers` | the traits a host drives a widget through (`MenuController`, `GraphController`, …) |

mod controllers;
mod events;
mod host;

pub use controllers::*;
pub use events::*;
pub use host::*;

/// Text justification for widget labels/content (shared by Button, cce-files' row
/// list, and settings; formerly defined by the dissolved json_layout host).
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Justification {
    Left,
    Center,
    Right,
}

/// A context-menu action dispatched on the menu's target widget (6bd phase 1: one enum
/// replaces the 13 per-action `WidgetHost` methods). `ClearText` is the search-box "Cear" item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
    /// Step the widget's own edit history (a text box's typing). Routed by
    /// the runner to the focused widget on the `undo` / `redo` chords before
    /// the app's `Application::undo` / `redo` get their turn; also reachable
    /// as "Undo" / "Redo" context-menu rows.
    Undo,
    Redo,
    ClearText,
    CopyKey,
    CopyValue,
    CopyPath,
    DeleteKey,
    ExpandNode,
    CollapseNode,
    ExpandAll,
    CollapseAll,
    /// Ramp: hide/show the bottom control strip, the graph claiming the space.
    ToggleRampControls,
}

use crate::colors;

use std::sync::atomic::AtomicUsize;

use std::collections::HashMap;

pub const DROPDOWN_ITEM_H: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId(pub usize);

pub static NEXT_WIDGET_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone)]
pub struct LayoutTree {
    pub parents: HashMap<WidgetId, WidgetId>,
    pub children: HashMap<WidgetId, Vec<WidgetId>>,
}

pub use crate::scene::paint::{ControlPlate, PlateStance};

pub use crate::widget::model::FocusRole;

pub use crate::context::UiContext;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConstraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl LayoutConstraints {
    pub fn new(min_w: f32, max_w: f32, min_h: f32, max_h: f32) -> Self {
        Self { min_width: min_w, max_width: max_w, min_height: min_h, max_height: max_h }
    }
    
    pub fn loose(max_w: f32, max_h: f32) -> Self {
        Self { min_width: 0.0, max_width: max_w, min_height: 0.0, max_height: max_h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

pub mod core;

pub mod input;

pub mod plate_dock;

pub mod container;

pub mod display;

pub mod editor;

pub mod shaping;

#[cfg(feature = "markdown")]
pub mod markdown;

/// An image a host has ready for a Markdown embed (`![[pic.png]]`): its id
/// from `vk::upload_rgba` and its size in px. `MarkdownView` and
/// `DocEditor` ask the host for one by the embed's link text — sizing at
/// layout, the id again at paint, so a re-upload after a reconnect needs
/// no relayout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbedImage {
    pub id: u32,
    pub width: u32,
    pub height: u32,
}

impl EmbedImage {
    /// The size it draws at in a column `max_w` wide: the requested width
    /// (else its own), the requested height (else the aspect's), scaled
    /// down as a whole to fit the column.
    pub fn fit(&self, want_w: Option<u32>, want_h: Option<u32>, max_w: f32) -> (f32, f32) {
        let (iw, ih) = (self.width.max(1) as f32, self.height.max(1) as f32);
        let w = want_w.map_or(iw, |w| w as f32);
        let h = want_h.map_or(w * ih / iw, |h| h as f32);
        if w > max_w {
            (max_w, h * max_w / w)
        } else {
            (w, h)
        }
    }
}

#[cfg(feature = "doc_editor")]
pub mod doc_editor;

pub mod line_edit;

pub mod model;

pub mod handle;

pub mod embedded;

pub mod scroll_region;

pub mod scroll_motion;

pub mod side_swipe;

// Re-exports
pub use self::editor::TextEditorState;

pub use self::line_edit::{EditOutcome, LineEdit};

pub use self::scroll_region::{ScrollRegion, ScrollbarActivity};

pub use self::side_swipe::{SideSwipe, SwipeDir};

pub use self::scroll_motion::{Bounds, ScrollAxis, ScrollMotion, ScrollPhase, ScrollSettings, LINE_PX};

pub use self::model::{Adapted, EventCtx, Input, Layout, Paint};

pub use self::handle::Handle;

pub use self::embedded::Embedded;

pub use self::core::{Widget, hover_animation, clipboard, context_menu, clear_widget_references};

pub use self::input::{
    Button, TextBox, Spinbox, Dropdown, Checkbox, Toggle, RadioGroup, Slider, RangeSlider,
    ColorSelector, Finger, Trackpad, get_font_db, ActiveThumb, FontSelector,
    BevelPreview, bevel_ease, parse_bevel_knobs, RampPreview,
    ButtonStrip, KeybindRecorder, Ramp, RampKey, ColorRamp, ColorRampKey,
    format_ramp_spec, parse_ramp_spec
};

pub use self::container::{
    Dialog, Group, GroupFrame,
    ContainerLayout, OverlayLayout, VerticalLayout, GridLayout, AdaptiveGridLayout,
    ColumnsLayout, MosaicLayout, ReverseMosaicLayout,
    ContentBg, ParametersBg,
    ScrollBox, MenuBar, SheetColumn, Spreadsheet, Breadcrumb,
    Paginator, TreeList, TreeElement
};

pub use self::display::{
    TextLabel, Label, StyledLabel, LabelPrim, TextItem, UsageBar,
    InfoBox, StatusDot, InteractiveListItem,
    GraphNode, Graph, TaggedQuad, node_wires, Float3, ProgressBar, StatusBar, Splitter, Separator,
    DotStatus, ImageView,
    truncate_head, truncate_tail,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    pub fn new(tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        Self { top_left: tl, top_right: tr, bottom_right: br, bottom_left: bl }
    }

    pub fn uniform(radius: f32) -> Self {
        Self::new(radius, radius, radius, radius)
    }
}
