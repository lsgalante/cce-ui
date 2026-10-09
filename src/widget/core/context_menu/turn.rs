//! Page rows and turns: a row that leads to a page, showing the page at the corner of the plate it
//! replaces, refilling it in place, and the animated turn between the two.

use super::*;

/// A page turn in progress — see [`TURN_MS`].
#[derive(Debug, Clone)]
pub(super) struct Turning {
    /// The plate as it stood when the turn began: its size and its rows.
    pub(super) from: Box<ContextMenuState>,
    pub(super) start: web_time::Instant,
    /// +1 forward (the page comes in from the right, where `›` points),
    /// -1 back.
    pub(super) dir: f32,
}

/// How strongly the rows a turn leaves and the rows it brings are drawn
/// at eased progress `e`: squared, so the two are seldom both legible at
/// once — the old ones mostly gone before the new ones mostly come.
pub(super) fn turn_fades(e: f32) -> (f32, f32) {
    ((1.0 - e) * (1.0 - e), e * e)
}

impl ContextMenuState {

    /// Show `options` as a PAGE, the plate's top-left at `(x, y)` — where
    /// the plate it turns from stood — with a back band reading `‹ back`
    /// across its top when `back` names that plate. The rows are new
    /// rows (sliders and page rows are set again after, as after
    /// [`show`](Self::show)); what changes is that a placement keeps the
    /// corner rather than opening from a pointer.
    ///
    /// The turn is ANIMATED from the plate that stood there — up, or
    /// hidden in the same moment (`TURN_HANDOFF`) — forward for a page
    /// with a back band, back for one without (a menu returned to).
    pub fn show_page(&mut self, x: f32, y: f32, back: Option<&str>, options: Vec<String>, header_count: usize, target: WidgetId) {
        const TURN_HANDOFF: std::time::Duration = std::time::Duration::from_millis(100);
        let from = (self.visible || self.hidden_at.is_some_and(|t| t.elapsed() < TURN_HANDOFF)).then(|| {
            let mut from = self.clone();
            from.turning = None;
            from.hovered_item = None;
            from.back_hovered = false;
            Box::new(from)
        });
        self.show(x, y, options, header_count, target);
        self.turned = true;
        self.turning = from.map(|from| Turning {
            from,
            start: web_time::Instant::now(),
            dir: if back.is_some() { 1.0 } else { -1.0 },
        });
        if let Some(title) = back {
            let (family, size) = label_font();
            let need = PAD + chevron_size(size) + MARK_GAP + crate::widget::display::measure_text_width(title, &family, size) + PAD;
            self.w = self.w.max(need);
            self.back = Some(title.to_string());
            self.content_h += ROW_H;
            self.h = self.content_h;
        }
    }

    /// New labels and slider values for the rows that are showing, in
    /// place — the hover, the scroll, the back band, the page rows and a
    /// held slider are kept — which is how a host re-marks a row of a
    /// menu that stays up after the row ran. `false`, and nothing
    /// changed, when the number of rows differs: that is another menu,
    /// to be shown afresh.
    pub fn refill(&mut self, options: Vec<String>, sliders: &[Option<MenuSlider>]) -> bool {
        if options.len() != self.options.len() {
            return false;
        }
        self.options = options;
        for i in 0..self.options.len() {
            // A held slider is the pointer's until the release.
            if self.slider_drag == Some(i) {
                continue;
            }
            self.sliders[i] = sliders.get(i).copied().flatten();
        }
        true
    }

    /// Animate the shown menu as turning from a plate of `w` x `h` at its
    /// corner that had no rows of its own to show going — what a host
    /// turning back from a plate the menu does not draw (a dialog) asks
    /// for. `forward` as for [`Self::show_page`].
    pub fn turn_from_size(&mut self, w: f32, h: f32, forward: bool) {
        let mut from = self.clone();
        from.turning = None;
        from.options.clear();
        from.sliders.clear();
        from.pages.clear();
        from.back = None;
        from.hovered_item = None;
        from.w = w;
        from.h = h;
        self.turning = Some(Turning { from: Box::new(from), start: web_time::Instant::now(), dir: if forward { 1.0 } else { -1.0 } });
    }

    /// How far the turn in progress has gone, eased, 0..1; `None` when
    /// there is none or it has finished.
    pub fn turn_progress(&self) -> Option<f32> {
        let t = self.turning.as_ref()?;
        let raw = t.start.elapsed().as_secs_f32() * 1000.0 / turn_ms();
        (raw < 1.0).then(|| turn_ease(raw))
    }

    /// The rect the plate is drawn at: its own, or on its way there from
    /// the one it turned from.
    pub fn drawn_rect(&self) -> crate::scene::layout::Rect {
        let mut r = crate::scene::layout::Rect { x: self.x, y: self.y, width: self.w, height: self.h };
        if let (Some(e), Some(t)) = (self.turn_progress(), self.turning.as_ref()) {
            r.width = t.from.w + (self.w - t.from.w) * e;
            r.height = t.from.h + (self.h - t.from.h) * e;
        }
        r
    }

    /// The size a host surface has to give the plate: its own, or while
    /// it turns, the larger of the two it turns between.
    pub fn surface_size(&self) -> (f32, f32) {
        match (self.turn_progress(), self.turning.as_ref()) {
            (Some(_), Some(t)) => (self.w.max(t.from.w), self.content_h.max(t.from.h)),
            _ => (self.w, self.content_h),
        }
    }

    /// The band's height above the rows: one row on a page that goes
    /// back somewhere, none otherwise.
    pub(super) fn band_h(&self) -> f32 {
        if self.back.is_some() { ROW_H } else { 0.0 }
    }

    /// The top of the back band, where it is drawn.
    pub fn back_band_y(&self) -> f32 {
        self.y + PAD - self.scroll
    }

    /// Whether `(px, py)` is on the back band.
    pub fn on_back_band(&self, px: f32, py: f32) -> bool {
        if self.back.is_none() || !self.hit_test(px, py) {
            return false;
        }
        let top = self.back_band_y();
        py >= top && py < top + ROW_H
    }

    /// The turn a press at `(px, py)` asks for: the back band goes back,
    /// a page row goes into its page.
    pub fn turn_at(&self, px: f32, py: f32) -> Option<PageTurn> {
        if !self.visible {
            return None;
        }
        if self.on_back_band(px, py) {
            return Some(PageTurn::Back);
        }
        self.row_at(px, py).filter(|&i| self.leads_to_page(i)).map(PageTurn::Into)
    }

    /// The turn a side swipe asked for since the last call.
    pub fn take_turn(&mut self) -> Option<PageTurn> {
        self.turn.take()
    }
}
