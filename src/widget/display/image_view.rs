//! `ImageView` — a GPU-textured image fitted into the widget's rect via
//! [`fit_rect`]. The view BORROWS its image id: ids come from
//! [`crate::vk::upload_rgba`] and stay owned by the app, which frees them with
//! [`crate::vk::free_image`] when done — the widget never uploads or frees GPU
//! resources, so one id can back several views and a dropped view leaks
//! nothing. `image: None` paints only the optional letterbox floor.
//!
//! **Borrowing it means the owner has to replace it when the renderer is
//! rebuilt.** An image id names an entry in one renderer's image table, and a
//! renderer does not outlive its session: `window_runner` builds a new one
//! around the same `Application` when it repairs a lost Wayland transport. A
//! draw for an id the new table does not hold is skipped rather than
//! reported, so a view left holding a pre-reconnect id goes blank and stays
//! blank, with nothing logged. Set the image again from
//! [`Application::renderer_init`] on every renderer after the first — the
//! app is the only party that can produce those pixels a second time.
//! ([`crate::upload_icon`] is the one exception, for bundled glyphs: it
//! re-resolves itself, and `Button::with_icon_name` is how a widget opts into
//! that.)
//!
//! [`Application::renderer_init`]: crate::engine::Application::renderer_init

use crate::scene::layout::{fit_rect, FitMode, Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

#[derive(Debug, Clone)]
pub struct ImageView {
    /// (image id, native width, native height).
    pub image: Option<(u32, u32, u32)>,
    pub fit: FitMode,
    pub alpha: f32,
    /// Floor painted across the whole rect behind the fitted image (the
    /// letterbox bars); `None` paints no floor.
    pub bg: Option<[f32; 4]>,
}

impl ImageView {
    pub fn new() -> Adapted<ImageView> {
        Adapted::new(ImageView {
            image: None,
            fit: FitMode::Contain { max_upscale: 4.0 },
            alpha: 1.0,
            bg: None,
        })
    }

    pub fn set_image(&mut self, image: Option<(u32, u32, u32)>) {
        self.image = image;
    }
}

/// By-value builders can't flow through `Deref`, so they live on the wrapped
/// type (the `UsageBar::with_colors` idiom).
impl Adapted<ImageView> {
    pub fn with_image(mut self, id: u32, width: u32, height: u32) -> Self {
        self.image = Some((id, width, height));
        self
    }

    pub fn with_fit(mut self, fit: FitMode) -> Self {
        self.fit = fit;
        self
    }

    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    pub fn with_bg(mut self, bg: [f32; 4]) -> Self {
        self.bg = Some(bg);
        self
    }
}

impl Layout for ImageView {
    /// Native pixel dimensions; the container may still assign any rect —
    /// `fit` decides how the image maps into it.
    fn intrinsic_size(&self) -> Option<Size> {
        self.image.map(|(_, w, h)| Size::new(w as f32, h as f32))
    }
}

impl Paint for ImageView {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        if let Some(bg) = self.bg {
            ctx.quad(rect, bg);
        }
        if let Some((id, w, h)) = self.image {
            let fitted = fit_rect(w, h, rect, self.fit);
            if fitted.width > 0.0 && fitted.height > 0.0 {
                ctx.image(id, fitted, self.alpha);
            }
        }
    }
}

impl Input for ImageView {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::paint::Prim;
    use crate::widget::{UiContext, WidgetHost};

    fn image_prims(view: &Adapted<ImageView>, ctx: &UiContext) -> Vec<(u32, Rect, f32)> {
        let mut pc = PaintCtx::new();
        crate::scene::painter::paint_root_into(ctx, view, &mut pc);
        pc.finish()
            .items
            .into_iter()
            .filter_map(|item| match item.prim {
                Prim::Image { image, rect, alpha } => Some((image, rect, alpha)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn paints_fitted_image_prim() {
        let ctx = UiContext::new();
        let mut view = ImageView::new().with_image(7, 50, 50);
        WidgetHost::set_rect(&mut view, 0.0, 0.0, 400.0, 400.0);
        let prims = image_prims(&view, &ctx);
        assert_eq!(prims.len(), 1);
        let (id, rect, alpha) = prims[0];
        assert_eq!(id, 7);
        assert_eq!(alpha, 1.0);
        // Contain caps at 4x: 200x200 centered in 400x400.
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (100.0, 100.0, 200.0, 200.0));
    }

    #[test]
    fn empty_view_paints_nothing_but_bg() {
        let ctx = UiContext::new();
        let mut view = ImageView::new().with_bg([0.1, 0.1, 0.1, 1.0]);
        WidgetHost::set_rect(&mut view, 0.0, 0.0, 100.0, 100.0);
        assert!(image_prims(&view, &ctx).is_empty());
    }

    #[test]
    fn intrinsic_size_is_native_dims() {
        let view = ImageView::new().with_image(1, 64, 40);
        let s = Layout::intrinsic_size(&*view).unwrap();
        assert_eq!((s.width, s.height), (64.0, 40.0));
    }
}
