pub mod button;
pub mod checkbox;
pub mod slider;
pub mod slider2d;
pub mod spinbox;
pub mod color_selector;
pub mod dropdown;
pub mod text_box;
pub mod trackpad;
pub mod font_selector;
pub mod button_strip;
pub mod keybind_recorder;
pub mod ramp;
pub mod bevel_preview;
pub mod ramp_preview;

pub use button::{Button, ButtonKind, PageButton};
pub use checkbox::{Checkbox, Toggle};
pub use slider::{Slider, RangeSlider, ActiveThumb};
pub use slider2d::Slider2D;
pub use spinbox::Spinbox;
pub use color_selector::ColorSelector;
pub use dropdown::Dropdown;
pub use text_box::{TextBox, get_font_db};
pub use trackpad::{Trackpad, Finger};
pub use font_selector::FontSelector;
pub use button_strip::ButtonStrip;
pub use keybind_recorder::KeybindRecorder;
pub use ramp::{Ramp, RampKey, ColorRamp, ColorRampKey, format_ramp_spec, parse_ramp_spec};
pub use bevel_preview::{BevelPreview, bevel_ease, parse_bevel_knobs};
pub use ramp_preview::RampPreview;

// `BREADCRUMB_PADDING` / `SEGMENT_GAP` lived here and had exactly one consumer
// between them. The breadcrumb owns its own spacing now (`Breadcrumb::SEG_INSET`,
// `SEG_GAP`), where it sits next to the geometry that has to agree with it.
