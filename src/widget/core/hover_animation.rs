use std::cell::RefCell;

#[derive(Debug, Clone)]
pub struct HoverState {
    pub current_x: f32,
    pub current_y: f32,
    pub current_w: f32,
    pub current_h: f32,
    pub current_alpha: f32,

    pub target_x: Option<f32>,
    pub target_y: Option<f32>,
    pub target_w: Option<f32>,
    pub target_h: Option<f32>,
    pub target_alpha: f32,

    pub registered_this_frame: bool,
    pub scroll_offset: f32,
}

impl HoverState {
    pub fn new() -> Self {
        Self {
            current_x: 0.0,
            current_y: 0.0,
            current_w: 0.0,
            current_h: 0.0,
            current_alpha: 0.0,

            target_x: None,
            target_y: None,
            target_w: None,
            target_h: None,
            target_alpha: 0.0,

            registered_this_frame: false,
            scroll_offset: 0.0,
        }
    }
}

// The highlight and the cursor it follows are the current window's
// (`crate::window_state`), not the thread's.
fn hover_state<R>(f: impl FnOnce(&RefCell<HoverState>) -> R) -> R {
    crate::window_state::with(|w| f(&w.hover))
}
fn cursor_state<R>(f: impl FnOnce(&RefCell<(f32, f32)>) -> R) -> R {
    crate::window_state::with(|w| f(&w.cursor))
}

pub fn set_cursor_pos(x: f32, y: f32) {
    cursor_state(|pos| {
        *pos.borrow_mut() = (x, y);
    });
}

pub fn reset_frame_registration() {
    hover_state(|state| {
        state.borrow_mut().registered_this_frame = false;
    });
}

pub fn set_scroll_offset(offset: f32) {
    hover_state(|state| {
        state.borrow_mut().scroll_offset = offset;
    });
}

pub fn get_scroll_offset() -> f32 {
    hover_state(|state| {
        state.borrow().scroll_offset
    })
}

pub fn register_hovered(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    hover_state(|state| {
        let mut s = state.borrow_mut();
        s.target_x = Some(x);
        s.target_y = Some(y);
        s.target_w = Some(w);
        s.target_h = Some(h);
        s.target_alpha = color[3];
        s.registered_this_frame = true;
    });
}

pub fn post_render_check() {
    hover_state(|state| {
        let mut s = state.borrow_mut();
        if !s.registered_this_frame {
            s.target_alpha = 0.0;
            let (cx, cy) = cursor_state(|pos| *pos.borrow());
            s.target_x = Some(cx);
            s.target_y = Some(cy + s.scroll_offset);
            s.target_w = Some(0.0);
            s.target_h = Some(0.0);
        }
    });
}

pub fn tick(dt: f32) -> bool {
    hover_state(|state| {
        let mut s = state.borrow_mut();
        let decay = 15.0;
        // The per-tick approach fraction; 1 lands on the target at once,
        // which is the whole of animations-off for the highlight.
        let k = if crate::motion::enabled() { 1.0 - (-decay * dt).exp() } else { 1.0 };
        let mut changed = false;

        if s.current_alpha <= 0.001 && s.target_alpha > 0.0 {
            if let (Some(tx), Some(ty), Some(tw), Some(th)) = (s.target_x, s.target_y, s.target_w, s.target_h) {
                s.current_x = tx;
                s.current_y = ty;
                s.current_w = tw;
                s.current_h = th;
            }
        }

        if (s.current_alpha - s.target_alpha).abs() > 0.001 {
            s.current_alpha += (s.target_alpha - s.current_alpha) * k;
            changed = true;
        } else if s.current_alpha != s.target_alpha {
            s.current_alpha = s.target_alpha;
            changed = true;
        }

        if let (Some(tx), Some(ty), Some(tw), Some(th)) = (s.target_x, s.target_y, s.target_w, s.target_h) {
            if (s.current_x - tx).abs() > 0.1 {
                s.current_x += (tx - s.current_x) * k;
                changed = true;
            } else if s.current_x != tx {
                s.current_x = tx;
                changed = true;
            }

            if (s.current_y - ty).abs() > 0.1 {
                s.current_y += (ty - s.current_y) * k;
                changed = true;
            } else if s.current_y != ty {
                s.current_y = ty;
                changed = true;
            }

            if (s.current_w - tw).abs() > 0.1 {
                s.current_w += (tw - s.current_w) * k;
                changed = true;
            } else if s.current_w != tw {
                s.current_w = tw;
                changed = true;
            }

            if (s.current_h - th).abs() > 0.1 {
                s.current_h += (th - s.current_h) * k;
                changed = true;
            } else if s.current_h != th {
                s.current_h = th;
                changed = true;
            }
        }

        changed
    })
}

pub fn get_quad() -> Option<(f32, f32, f32, f32, [f32; 4])> {
    hover_state(|state| {
        let s = state.borrow();
        if s.current_alpha > 0.001 {
            Some((
                s.current_x,
                s.current_y,
                s.current_w,
                s.current_h,
                [1.0, 1.0, 1.0, s.current_alpha],
            ))
        } else {
            None
        }
    })
}
