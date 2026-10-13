//! Placing a widget in a host's block ([`place`] / [`place_h`]): laid out at the block, shaped,
//! its open popover registered. The host paints it there with `widget::painter::paint_root_into`.

use crate::context::UiContext;
use crate::widget::WidgetHost;

/// [`place`] for a widget the context owns, named by its handle: lent for the call.
/// Places nothing if the widget is gone or already out on loan.
pub fn place_h<T: WidgetHost + 'static>(h: crate::widget::Handle<T>, x: f32, y: f32, ww: f32, wh: f32, ctx: &mut UiContext) {
    ctx.lend_h(h, |w, ctx| place(w, x, y, ww, wh, ctx));
}

/// Place `w` in a host's block: `(x, y)` is the top of the detached label, `wh` the block's
/// height, label strip included, and `ww` its width — the block `set_rect` takes. The widget
/// is laid out at exactly that size, shaped (`prepare_text`), and its open popover, if it has
/// one, registered with the context. The host then paints it where it was placed with
/// `widget::painter::paint_root_into`, as `compose::Form` does its widgets.
pub fn place<T: WidgetHost + 'static>(w: &mut T, x: f32, y: f32, ww: f32, wh: f32, ctx: &mut UiContext) {
    // `layout` takes the CONTENT origin and height, so step down by the strip.
    let strip = w.label_strip();
    let content_h = (wh - strip).max(0.0);
    w.layout(crate::widget::Point { x, y: y + strip }, crate::widget::LayoutConstraints::new(ww, ww, content_h, content_h), ctx);

    // Shape: `build_frame` does it for the registered widgets of a frame, but a widget
    // painted from a page buffer may be painted before that pass. `prepare_text` is where a
    // TextBox records the per-glyph x offsets its selection highlight, caret and
    // click->index mapping read; without it they fall back to an inked extent and the
    // highlight under-runs the glyphs. Hosts that shape for themselves just re-read the
    // shared buffer cache here.
    if let Ok(mut fs) = crate::geometry_font_system().lock() {
        w.prepare_text(&mut fs);
    }
    // Painting reads no popover registration, so registering before the paint changes
    // nothing.
    if w.popover_rect().is_some() {
        ctx.register_popover_id(w.base().id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::paint::{PaintCtx, Prim};
    use crate::widget::painter::paint_root_into;
    use crate::widget::{Button, Dropdown, TextBox, WidgetHostExt};

    /// A placed widget takes exactly the block it was given and paints in it.
    #[test]
    fn a_placed_button_takes_its_block_and_paints_there() {
        let mut ctx = UiContext::new();
        let b = ctx.insert(Button::new(0.0, 0.0, 0.0, 0.0).with_label("OK"));
        place_h(b, 10.0, 20.0, 80.0, 28.0, &mut ctx);
        assert_eq!(ctx[b].rect(), (10.0, 20.0, 80.0, 28.0), "the block it was given");
        let mut pc = PaintCtx::new();
        paint_root_into(&ctx, &ctx[b], &mut pc);
        let (x, y) = pc
            .finish()
            .items
            .iter()
            .find_map(|i| match i.prim {
                Prim::Text { x, y, .. } => Some((x, y)),
                _ => None,
            })
            .expect("the label is painted");
        assert!((10.0..90.0).contains(&x) && (20.0..48.0).contains(&y), "label at ({x}, {y})");
    }

    /// The label strip heads the block: the field is laid out below it.
    #[test]
    fn a_placed_labelled_text_box_lays_its_field_out_below_its_label() {
        let mut ctx = UiContext::new();
        let t = ctx.insert(TextBox::new("hello".to_string()).with_label("Name"));
        let strip = ctx[t].label_strip();
        assert!(strip > 0.0, "a labelled box carries a strip");
        place_h(t, 10.0, 20.0, 160.0, 48.0, &mut ctx);
        assert_eq!(ctx[t].rect(), (10.0, 20.0, 160.0, 48.0), "the block it was given");
        let field = ctx[t].content_rect();
        assert_eq!((field.y, field.height), (20.0 + strip, 48.0 - strip));
    }

    /// An open popover is registered with the context as the widget is placed; a closed one
    /// is not.
    #[test]
    fn placing_an_open_dropdown_registers_its_popover() {
        let mut ctx = UiContext::new();
        let mut open = Dropdown::new(vec!["a".to_string(), "b".to_string()], 0);
        open.inner_mut().open = true;
        let open = ctx.insert(open);
        let closed = ctx.insert(Dropdown::new(vec!["a".to_string()], 0));
        place_h(closed, 0.0, 0.0, 120.0, 24.0, &mut ctx);
        assert!(ctx.active_popovers.is_empty());
        place_h(open, 0.0, 40.0, 120.0, 24.0, &mut ctx);
        assert_eq!(ctx.active_popovers, vec![ctx[open].id()]);
    }
}
