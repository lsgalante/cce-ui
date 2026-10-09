//! The narrow widget traits, and the adapter that makes a widget a host.
//!
//! A widget's behaviour is three independent traits, one per concern: [`Layout`] (its size, its
//! label's place, its children's arrangement, the children it embeds), [`Paint`] (what it draws
//! given its laid-out rect, read by the paint walk in [`crate::scene::painter`]) and [`Input`]
//! (its hit, events, focus role, values and drags). [`Adapted<W>`] wraps a widget that implements
//! all three and is the one production implementor of [`WidgetHost`], the surface the machinery
//! sees: it carries the [`Widget`] base (rect, id, label, dirty) and the visibility flag, and
//! forwards each host method to the trait that answers it.
//!
//! The traits are not supertraits of `WidgetHost`, on purpose: a supertrait's methods are always
//! in scope on the subtrait, so `elem.children()` on a `&dyn WidgetHost` turned ambiguous, and a
//! blanket `impl<T: WidgetHost> Paint for T` does not let `&dyn WidgetHost` coerce to `&dyn Paint`.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | `Adapted`: the struct, `Drop`, `Default`, `Deref` to the widget |
//! | `layout` | the [`Layout`] trait |
//! | `paint` | the [`Paint`] trait |
//! | `input` | the [`Input`] trait, [`FocusRole`], and [`EventCtx`] |
//! | `adapted` | `Adapted`'s inherent API: construction, labels, values, direct event entry points |
//! | `host` | `impl WidgetHost for Adapted` |

mod adapted;
mod host;
mod input;
mod layout;
mod paint;
#[cfg(test)]
mod tests;

pub use input::{EventCtx, FocusRole, Input};
pub use layout::Layout;
pub use paint::Paint;

use crate::scene::layout::{Rect, Size};
use crate::scene::paint::{PaintCtx, Prim};
use crate::widget::{WidgetHost, Event, TextLabel, UiContext, Widget, WidgetId, WidgetHostExt};

/// Wraps a narrow-trait widget `W` so it lives in the legacy `*mut dyn WidgetHost` tree. Carries the
/// [`Widget`] base that `WidgetHost`'s rect / id / dirty machinery needs, and forwards the concern
/// methods to `W`. See the module docs for why this bridge exists rather than a supertrait split.
///
/// The bounds live on the struct (not just the `WidgetHost` impl) so `Drop` can clear the global
/// focus / context-menu references through `&dyn WidgetHost` — the same guard legacy widgets with
/// `Drop` impls (e.g. the old `Checkbox`) carried.
#[derive(Debug, Clone)]
pub struct Adapted<W: Layout + Paint + Input + 'static> {
    base: Widget,
    /// The [`Widget`] base carries no visibility, and the legacy `WidgetHost` defaults are a no-op
    /// `set_visible` + always-true `visible()` — every hideable legacy widget stores its own
    /// flag. The adapter owns it once for all migrated widgets: hosts toggle panes through
    /// `WidgetHost::set_visible` (the designer), and the hit-test/render bridges gate on it.
    visible: bool,
    inner: W,
}

impl<W: Layout + Paint + Input + 'static> Drop for Adapted<W> {
    fn drop(&mut self) {
        crate::widget::clear_widget_references(self);
    }
}

/// Plain-data widgets constructed via `Default` (PreviewState in cce-files) keep their
/// construction sites when the wrapper lands.
impl<W: Layout + Paint + Input + Default + 'static> Default for Adapted<W> {
    fn default() -> Self {
        Adapted::new(W::default())
    }
}

/// Auto-deref to the wrapped widget, so call sites keep using a migrated widget's own state and
/// methods directly (`dot.status`, `dot.set_status(..)`) without knowing about the wrapper.
/// (By-value builders can't flow through `Deref` — those get mirrored per-widget, like
/// `with_label` here or `UsageBar::with_colors`.)
impl<W: Layout + Paint + Input + 'static> std::ops::Deref for Adapted<W> {
    type Target = W;
    fn deref(&self) -> &W {
        &self.inner
    }
}

impl<W: Layout + Paint + Input + 'static> std::ops::DerefMut for Adapted<W> {
    fn deref_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}
