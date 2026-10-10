//! `DatePicker` — a month grid for choosing a day, drawn by the host over
//! its own content (an immediate-mode helper like `Checkbox::paint_inline`,
//! not a `WidgetHost` widget). The host opens it beside the thing being
//! dated, routes the pointer and keys to it while it is open, and acts on
//! the [`Outcome`] each press or key answers; it paints it last.
//!
//! The surface is the dropdown's: the popover material on a carved plate.
//! Inside, the item's name (optional), ‹ month year ›, the weekday row, six
//! weeks of days — today ringed, the current date filled — and Today /
//! Tomorrow / Clear. Keys: arrows move the day (crossing months), PageUp /
//! PageDown a month, Home today, Enter picks, Delete clears, Escape closes.
//! A host flips months with the wheel through [`DatePicker::shift_month`].
//!
//! Display-list text draws above all geometry, and only a registered
//! widget's popover gets the engine's text clamp, so a host hides its own
//! text under [`DatePicker::rect`] while the picker is open.

use chrono::{Datelike, Days, Months, NaiveDate, Weekday};

use crate::scene::layout::Rect;
use crate::scene::paint::{AlignH, AlignV, PaintCtx, TextAttrs, TextLayout};
use crate::widget::{Key, KeyEvent, NamedKey};

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Prev,
    Next,
    Day(NaiveDate),
    Today,
    Tomorrow,
    Clear,
    /// Inside the picker but on nothing that acts.
    Inside,
}

/// What the picker asks of its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Date the item with this day; the picker is done.
    Set(NaiveDate),
    /// Take the item's date away; the picker is done.
    Clear,
    /// Close without a change.
    Close,
    /// The picker changed (month, cursor): redraw it.
    Redraw,
    /// Not the picker's.
    Ignored,
}

#[derive(Clone, Debug)]
pub struct DatePicker {
    pub rect: Rect,
    pub hover: Option<Hit>,
    /// The first of the month shown.
    month: NaiveDate,
    /// The date the item had when the picker opened.
    current: Option<NaiveDate>,
    /// The keyboard's day: arrows move it, Enter picks it.
    cursor: NaiveDate,
    title: Option<String>,
    week_start: Weekday,
    /// Whether Clear acts (a date that must stay — an event's — has none).
    clearable: bool,
}

/// The picker's measures, from the toolkit's getters: a day cell is a
/// control high, and the content stands a text inset in from the carved
/// rim.
struct Metrics {
    cell: f32,
    pad: f32,
    title_h: f32,
    week_h: f32,
}

impl Metrics {
    fn get() -> Metrics {
        let cell = crate::layout::dropdown_height();
        Metrics {
            cell,
            pad: crate::layout::bevel_width() + crate::layout::CONTROL_TEXT_INSET / 2.0,
            title_h: cell * 0.8,
            week_h: cell * 0.75,
        }
    }
}

fn first_of(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

fn month_name(month: u32) -> String {
    match month {
        1 => crate::l10n::tr("date-picker-january"),
        2 => crate::l10n::tr("date-picker-february"),
        3 => crate::l10n::tr("date-picker-march"),
        4 => crate::l10n::tr("date-picker-april"),
        5 => crate::l10n::tr("date-picker-may"),
        6 => crate::l10n::tr("date-picker-june"),
        7 => crate::l10n::tr("date-picker-july"),
        8 => crate::l10n::tr("date-picker-august"),
        9 => crate::l10n::tr("date-picker-september"),
        10 => crate::l10n::tr("date-picker-october"),
        11 => crate::l10n::tr("date-picker-november"),
        _ => crate::l10n::tr("date-picker-december"),
    }
}

fn weekday_name(day: Weekday) -> String {
    match day {
        Weekday::Mon => crate::l10n::tr("date-picker-mon"),
        Weekday::Tue => crate::l10n::tr("date-picker-tue"),
        Weekday::Wed => crate::l10n::tr("date-picker-wed"),
        Weekday::Thu => crate::l10n::tr("date-picker-thu"),
        Weekday::Fri => crate::l10n::tr("date-picker-fri"),
        Weekday::Sat => crate::l10n::tr("date-picker-sat"),
        Weekday::Sun => crate::l10n::tr("date-picker-sun"),
    }
}

impl DatePicker {
    /// Its size: seven cells wide; title, header, weekdays, six weeks and
    /// the footer high, all inside the padding.
    pub fn size() -> (f32, f32) {
        let m = Metrics::get();
        let w = 7.0 * m.cell + 2.0 * m.pad;
        let h = m.pad + m.title_h + m.cell + m.week_h + 6.0 * m.cell + m.pad / 2.0 + m.cell + m.pad;
        (w, h)
    }

    /// Open below `anchor` (what is being dated), its right edge at
    /// `right`, flipped above when there is no room below, and kept inside
    /// a window of size `win`. It shows the month of `current`, else of
    /// `today`.
    pub fn open(anchor: Rect, right: f32, current: Option<NaiveDate>, today: NaiveDate, win: (f32, f32)) -> Self {
        let (w, h) = Self::size();
        let edge = Metrics::get().pad;
        let x = (right - w).min(win.0 - w - edge).max(edge.min(win.0 - w)).max(0.0);
        let below = anchor.y + anchor.height;
        let y = if below + h <= win.1 || anchor.y - h < 0.0 { below } else { anchor.y - h };
        let y = y.clamp(0.0, (win.1 - h).max(0.0));
        let cursor = current.unwrap_or(today);
        DatePicker {
            rect: Rect { x, y, width: w, height: h },
            hover: None,
            month: first_of(cursor),
            current,
            cursor,
            title: None,
            week_start: Weekday::Mon,
            clearable: true,
        }
    }

    /// Name what is being dated on the picker's first line — it may cover
    /// its own row.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The weekday the grid's columns start on (Monday by default).
    pub fn with_week_start(mut self, day: Weekday) -> Self {
        self.week_start = day;
        self
    }

    /// No Clear: for a date that must stay.
    pub fn without_clear(mut self) -> Self {
        self.clearable = false;
        self
    }

    // ── geometry ──────────────────────────────────────────────────────────

    fn head(&self, m: &Metrics) -> Rect {
        Rect { x: self.rect.x + m.pad, y: self.rect.y + m.pad + m.title_h, width: 7.0 * m.cell, height: m.cell }
    }

    fn prev_rect(&self, m: &Metrics) -> Rect {
        let h = self.head(m);
        Rect { width: m.cell, ..h }
    }

    fn next_rect(&self, m: &Metrics) -> Rect {
        let h = self.head(m);
        Rect { x: h.x + h.width - m.cell, width: m.cell, ..h }
    }

    fn grid_top(&self, m: &Metrics) -> f32 {
        self.rect.y + m.pad + m.title_h + m.cell + m.week_h
    }

    /// The grid's first cell: the week-start day on or before the 1st.
    fn first_cell(&self) -> NaiveDate {
        let back = (self.month.weekday().num_days_from_monday() + 7 - self.week_start.num_days_from_monday()) % 7;
        self.month.checked_sub_days(Days::new(back as u64)).unwrap_or(self.month)
    }

    fn day_rect(&self, m: &Metrics, i: usize) -> Rect {
        Rect {
            x: self.rect.x + m.pad + (i % 7) as f32 * m.cell,
            y: self.grid_top(m) + (i / 7) as f32 * m.cell,
            width: m.cell,
            height: m.cell,
        }
    }

    /// The footer's three buttons: Today, Tomorrow, Clear — two, three and
    /// two cells wide, the longest word in the widest.
    fn foot_rects(&self, m: &Metrics) -> [Rect; 3] {
        let y = self.grid_top(m) + 6.0 * m.cell + m.pad / 2.0;
        let x0 = self.rect.x + m.pad;
        let r = |from: f32, cells: f32| Rect { x: x0 + from * m.cell, y, width: cells * m.cell, height: m.cell };
        [r(0.0, 2.0), r(2.0, 3.0), r(5.0, 2.0)]
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.rect.contains(x, y)
    }

    /// What `(x, y)` is over; `None` outside the picker.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        if !self.contains(x, y) {
            return None;
        }
        let m = Metrics::get();
        if self.prev_rect(&m).contains(x, y) {
            return Some(Hit::Prev);
        }
        if self.next_rect(&m).contains(x, y) {
            return Some(Hit::Next);
        }
        if let Some(i) = (0..42).find(|&i| self.day_rect(&m, i).contains(x, y)) {
            return Some(self.first_cell().checked_add_days(Days::new(i as u64)).map_or(Hit::Inside, Hit::Day));
        }
        let [today, tomorrow, clear] = self.foot_rects(&m);
        Some(if today.contains(x, y) {
            Hit::Today
        } else if tomorrow.contains(x, y) {
            Hit::Tomorrow
        } else if clear.contains(x, y) {
            Hit::Clear
        } else {
            Hit::Inside
        })
    }

    // ── input ─────────────────────────────────────────────────────────────

    /// Show the month `delta` months away (the wheel, the chevrons).
    pub fn shift_month(&mut self, delta: i32) {
        let n = Months::new(delta.unsigned_abs());
        let moved = if delta < 0 { self.month.checked_sub_months(n) } else { self.month.checked_add_months(n) };
        if let Some(month) = moved {
            self.month = month;
        }
    }

    /// A left press on `hit`.
    pub fn press(&mut self, hit: Hit, today: NaiveDate) -> Outcome {
        match hit {
            Hit::Prev => {
                self.shift_month(-1);
                Outcome::Redraw
            }
            Hit::Next => {
                self.shift_month(1);
                Outcome::Redraw
            }
            Hit::Day(d) => Outcome::Set(d),
            Hit::Today => Outcome::Set(today),
            Hit::Tomorrow => today.succ_opt().map_or(Outcome::Ignored, Outcome::Set),
            Hit::Clear if self.clearable => Outcome::Clear,
            Hit::Clear | Hit::Inside => Outcome::Ignored,
        }
    }

    fn set_cursor(&mut self, day: Option<NaiveDate>) {
        if let Some(c) = day {
            self.cursor = c;
            self.month = first_of(c);
        }
    }

    /// A key press. Every key but these is `Ignored`.
    pub fn key(&mut self, event: &KeyEvent, today: NaiveDate) -> Outcome {
        let Key::Named(key) = &event.logical_key else { return Outcome::Ignored };
        let c = self.cursor;
        let moved = match key {
            NamedKey::Escape => return Outcome::Close,
            NamedKey::Enter => return Outcome::Set(c),
            NamedKey::Delete | NamedKey::Backspace if self.clearable => return Outcome::Clear,
            NamedKey::ArrowLeft => c.checked_sub_days(Days::new(1)),
            NamedKey::ArrowRight => c.checked_add_days(Days::new(1)),
            NamedKey::ArrowUp => c.checked_sub_days(Days::new(7)),
            NamedKey::ArrowDown => c.checked_add_days(Days::new(7)),
            NamedKey::PageUp => c.checked_sub_months(Months::new(1)),
            NamedKey::PageDown => c.checked_add_months(Months::new(1)),
            NamedKey::Home => Some(today),
            _ => return Outcome::Ignored,
        };
        self.set_cursor(moved);
        Outcome::Redraw
    }

    // ── paint ─────────────────────────────────────────────────────────────

    fn label(pc: &mut PaintCtx, text: String, rect: Rect, size: f32, color: [f32; 4], bold: bool) {
        let attrs = TextAttrs { weight: bold.then_some(600), ..Default::default() };
        let rgb = crate::color::to_srgb(color);
        let u8s = [(rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8];
        let layout = TextLayout { wrap_width: Some(rect.width), box_height: rect.height, align_h: AlignH::Center, align_v: AlignV::Middle };
        pc.text_boxed(text, rect.x, rect.y, size, u8s, None, None, attrs, layout);
    }

    pub fn paint(&self, pc: &mut PaintCtx, today: NaiveDate) {
        let m = Metrics::get();
        let radius = crate::layout::dropdown_corner_radius();
        let raw_bg = crate::color::dropdown_background_color();
        let base = if raw_bg[3] > 0.001 { raw_bg } else { crate::color::page_low_color() };
        let face = crate::scene::Material::popover(base);
        let depth = crate::layout::bevel_width().min(self.rect.height * 0.2);
        let (t, tr) = crate::layout::carve_inside(self.rect, (radius, radius, radius, radius), depth);
        pc.inset_plate(t, tr, Some(&face), depth);

        let size = crate::layout::control_label_font_detached_parsed().1;
        let small = (size * 0.85).round();
        let (fg, dim) = (crate::color::TEXT_FG, crate::color::TEXT_DIM);
        let faint = [dim[0] * 0.7, dim[1] * 0.7, dim[2] * 0.7, 1.0];
        let accent = crate::color::highlight_primary_color();
        let wash_color = crate::color::button_hover_color();
        let wash = |pc: &mut PaintCtx, r: Rect| pc.rounded_rect(r, r.height / 2.0, (true, true, true, true), wash_color);
        let hover = |h: Hit| self.hover == Some(h);

        if let Some(title) = &self.title {
            let r = Rect { x: self.rect.x + m.pad, y: self.rect.y + m.pad, width: 7.0 * m.cell, height: m.title_h };
            let rgb = crate::color::to_srgb(dim);
            pc.text_with(title.clone(), r.x, crate::layout::align_text_y(r.y, r.height, small, 0.0), small,
                [(rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8], None,
                Some([r.x, r.y, r.x + r.width, r.y + r.height]));
        }

        for (hit, r, icon) in [(Hit::Prev, self.prev_rect(&m), "chevron-left"), (Hit::Next, self.next_rect(&m), "chevron-right")] {
            if hover(hit) {
                wash(pc, r);
            }
            let s = (r.height * 0.55).round();
            let glyph = Rect { x: r.x + (r.width - s) / 2.0, y: r.y + (r.height - s) / 2.0, width: s, height: s };
            pc.icon(icon, glyph, crate::color::to_srgb(fg));
        }
        let h = self.head(&m);
        let (name, year) = (month_name(self.month.month()), self.month.year().to_string());
        let month = crate::l10n::tr_args("date-picker-month", &[("month", &name), ("year", &year)]);
        Self::label(pc, month, Rect { x: h.x + m.cell, width: h.width - 2.0 * m.cell, ..h }, size, fg, true);

        let mut day = self.week_start;
        for i in 0..7 {
            let r = Rect { x: self.rect.x + m.pad + i as f32 * m.cell, y: h.y + h.height, width: m.cell, height: m.week_h };
            Self::label(pc, weekday_name(day), r, small, faint, false);
            day = day.succ();
        }

        let first = self.first_cell();
        for i in 0..42 {
            let Some(d) = first.checked_add_days(Days::new(i as u64)) else { continue };
            let r = self.day_rect(&m, i);
            let inner = depth.min(2.0);
            let dot = Rect { x: r.x + inner, y: r.y + inner, width: r.width - 2.0 * inner, height: r.height - 2.0 * inner };
            let round = dot.height / 2.0;
            let chosen = self.current == Some(d);
            if chosen {
                pc.rounded_rect(dot, round, (true, true, true, true), accent);
            } else if hover(Hit::Day(d)) || self.cursor == d {
                wash(pc, dot);
            }
            if d == today && !chosen {
                pc.border(dot, (round, round, round, round), [0.0; 4], accent, 1.5);
            }
            let color = if chosen {
                [1.0, 1.0, 1.0, 1.0]
            } else if d.month() == self.month.month() {
                fg
            } else {
                faint
            };
            Self::label(pc, d.day().to_string(), r, small, color, d == today);
        }

        let [r_today, r_tomorrow, r_clear] = self.foot_rects(&m);
        for (hit, r, text) in [
            (Hit::Today, r_today, crate::l10n::tr("date-picker-today")),
            (Hit::Tomorrow, r_tomorrow, crate::l10n::tr("date-picker-tomorrow")),
            (Hit::Clear, r_clear, crate::l10n::tr("date-picker-clear")),
        ] {
            // Clear with nothing to clear (or not offered) stays in its place, faint.
            let inert = hit == Hit::Clear && (self.current.is_none() || !self.clearable);
            if hover(hit) && !inert {
                wash(pc, r);
            }
            Self::label(pc, text, r, small, if inert { faint } else { dim }, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at_origin(current: Option<NaiveDate>) -> DatePicker {
        let anchor = Rect { x: 0.0, y: 0.0, width: 600.0, height: 26.0 };
        DatePicker::open(anchor, 600.0, current, d(2026, 10, 10), (600.0, 900.0))
    }

    #[test]
    fn the_grid_starts_on_the_week_start_and_hits_map_to_days() {
        let p = at_origin(None);
        // October 2026 starts on a Thursday: a Monday grid opens on Sep 28.
        assert_eq!(p.first_cell(), d(2026, 9, 28));
        let m = Metrics::get();
        let r = p.day_rect(&m, 3);
        assert_eq!(p.hit(r.x + 2.0, r.y + 2.0), Some(Hit::Day(d(2026, 10, 1))));
        assert_eq!(p.clone().with_week_start(Weekday::Sun).first_cell(), d(2026, 9, 27));
        assert_eq!(p.hit(p.rect.x - 1.0, p.rect.y), None);
    }

    #[test]
    fn footer_presses_and_clear_only_when_offered() {
        let today = d(2026, 10, 10);
        let mut p = at_origin(Some(d(2026, 10, 15)));
        let m = Metrics::get();
        let [t, tm, c] = p.foot_rects(&m);
        assert_eq!(p.press(p.hit(t.x + 2.0, t.y + 2.0).unwrap(), today), Outcome::Set(today));
        assert_eq!(p.press(p.hit(tm.x + 2.0, tm.y + 2.0).unwrap(), today), Outcome::Set(d(2026, 10, 11)));
        assert_eq!(p.press(p.hit(c.x + 2.0, c.y + 2.0).unwrap(), today), Outcome::Clear);
        let mut fixed = p.without_clear();
        assert_eq!(fixed.press(Hit::Clear, today), Outcome::Ignored);
    }

    #[test]
    fn placement_flips_above_and_stays_in_the_window() {
        let (w, h) = DatePicker::size();
        let win = (w + 40.0, h + 60.0);
        let row = Rect { x: 8.0, y: win.1 - 40.0, width: win.0 - 16.0, height: 26.0 };
        let p = DatePicker::open(row, win.0 - 8.0, None, d(2026, 10, 10), win);
        assert!(p.rect.y >= 0.0 && p.rect.y + p.rect.height <= win.1);
        assert!(p.rect.x >= 0.0 && p.rect.x + p.rect.width <= win.0);
        assert!(p.rect.y + p.rect.height <= row.y, "flipped above its row");
    }

    #[test]
    fn keys_walk_days_across_months() {
        let today = d(2026, 10, 10);
        let mut p = at_origin(Some(d(2026, 10, 31)));
        let key = |k: NamedKey| KeyEvent {
            state: crate::widget::ElementState::Pressed,
            logical_key: Key::Named(k),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert_eq!(p.key(&key(NamedKey::ArrowRight), today), Outcome::Redraw);
        assert_eq!((p.cursor, p.month), (d(2026, 11, 1), d(2026, 11, 1)));
        p.key(&key(NamedKey::ArrowUp), today);
        assert_eq!(p.cursor, d(2026, 10, 25));
        assert_eq!(p.key(&key(NamedKey::Enter), today), Outcome::Set(d(2026, 10, 25)));
        assert_eq!(p.key(&key(NamedKey::Escape), today), Outcome::Close);
    }
}
