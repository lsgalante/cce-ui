use crate::widget::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

pub struct SplitBox {
    pub base: Widget,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub direction: SplitDirection,
    pub proportions: Vec<f32>,
    pub min_sizes: Vec<f32>,
    pub gap: f32,
    pub dragging_idx: Option<usize>,
    pub hovered_idx: Option<usize>,
}

impl SplitBox {
    pub fn new(direction: SplitDirection, gap: f32) -> Self {
        Self {
            base: Widget::new(),
            children: Vec::new(),
            parent: None,
            direction,
            proportions: Vec::new(),
            min_sizes: Vec::new(),
            gap,
            dragging_idx: None,
            hovered_idx: None,
        }
    }

    pub fn with_child(mut self, child: *mut (dyn Element + 'static), initial_proportion: f32, min_size: f32) -> Self {
        self.add_child_with_proportion(child, initial_proportion, min_size);
        self
    }

    pub fn add_child_with_proportion(&mut self, child: *mut (dyn Element + 'static), proportion: f32, min_size: f32) {
        self.children.push(child);
        self.proportions.push(proportion);
        self.min_sizes.push(min_size);
    }

    fn get_splitter_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let count = self.children.len();
        if count <= 1 {
            return Vec::new();
        }
        let (bx, by, bw, bh) = (self.base.x, self.base.y, self.base.w, self.base.h);
        let total_prop: f32 = self.proportions.iter().sum();
        if total_prop <= 0.0 {
            return Vec::new();
        }

        let mut rects = Vec::new();
        match self.direction {
            SplitDirection::Horizontal => {
                let total_gap = (count - 1) as f32 * self.gap;
                let child_space = (bw - total_gap).max(0.0);
                let mut curr_x = bx;
                for i in 0..(count - 1) {
                    let w_i = (self.proportions[i] / total_prop) * child_space;
                    let splitter_x = curr_x + w_i;
                    rects.push((splitter_x, by, self.gap, bh));
                    curr_x = splitter_x + self.gap;
                }
            }
            SplitDirection::Vertical => {
                let total_gap = (count - 1) as f32 * self.gap;
                let child_space = (bh - total_gap).max(0.0);
                let mut curr_y = by;
                for i in 0..(count - 1) {
                    let h_i = (self.proportions[i] / total_prop) * child_space;
                    let splitter_y = curr_y + h_i;
                    rects.push((bx, splitter_y, bw, self.gap));
                    curr_y = splitter_y + self.gap;
                }
            }
        }
        rects
    }
}

impl Element for SplitBox {
    crate::impl_widget_base!(SplitBox);

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        self.parent = parent;
        if parent.is_some() {
            let self_ptr = self as *mut Self;
            let self_id = self.base.id();
            unsafe {
                for &child in &self.children {
                    let child_id = (*child).base().unwrap().id();
                    ctx.register_widget(child_id, child);
                    ctx.link_ids(self_id, child_id);
                    (*child).set_parent(Some(self_ptr), ctx);
                }
            }
        }
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;

        let count = self.children.len();
        if count == 0 {
            return;
        }

        let total_prop: f32 = self.proportions.iter().sum();
        if total_prop <= 0.0 {
            return;
        }

        match self.direction {
            SplitDirection::Horizontal => {
                let total_gap = (count - 1) as f32 * self.gap;
                let child_space = (w - total_gap).max(0.0);
                let mut curr_x = x;
                for i in 0..count {
                    let w_i = (self.proportions[i] / total_prop) * child_space;
                    unsafe {
                        (*self.children[i]).set_rect(curr_x, y, w_i, h);
                    }
                    curr_x += w_i + self.gap;
                }
            }
            SplitDirection::Vertical => {
                let total_gap = (count - 1) as f32 * self.gap;
                let child_space = (h - total_gap).max(0.0);
                let mut curr_y = y;
                for i in 0..count {
                    let h_i = (self.proportions[i] / total_prop) * child_space;
                    unsafe {
                        (*self.children[i]).set_rect(x, curr_y, w, h_i);
                    }
                    curr_y += h_i + self.gap;
                }
            }
        }
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut handled = false;
        
        // Check dragging
        if let Some(idx) = self.dragging_idx {
            let total_prop: f32 = self.proportions.iter().sum();
            if total_prop > 0.0 {
                let splitters = self.get_splitter_rects();
                if idx < splitters.len() {
                    match self.direction {
                        SplitDirection::Horizontal => {
                            
                            // Find combine range of idx and idx+1
                            let left_child_x = if idx == 0 { self.base.x } else {
                                unsafe {
                                    let r = (*self.children[idx]).rect();
                                    r.0
                                }
                            };
                            let right_child_end = unsafe {
                                let r = (*self.children[idx + 1]).rect();
                                r.0 + r.2
                            };
                            
                            let combined_w = (right_child_end - left_child_x - self.gap).max(0.0);
                            let min_left = self.min_sizes.get(idx).cloned().unwrap_or(20.0);
                            let min_right = self.min_sizes.get(idx + 1).cloned().unwrap_or(20.0);
                            
                            let new_left_w = (px - left_child_x - self.gap / 2.0).clamp(min_left, combined_w - min_right);
                            
                            let comb_prop = self.proportions[idx] + self.proportions[idx + 1];
                            if combined_w > 0.1 {
                                self.proportions[idx] = (new_left_w / combined_w) * comb_prop;
                                self.proportions[idx + 1] = comb_prop - self.proportions[idx];
                                let (bx, by, bw, bh) = (self.base.x, self.base.y, self.base.w, self.base.h);
                                self.set_rect(bx, by, bw, bh);
                                self.mark_dirty(ctx);
                                handled = true;
                            }
                        }
                        SplitDirection::Vertical => {
                            
                            // Find combine range of idx and idx+1
                            let top_child_y = if idx == 0 { self.base.y } else {
                                unsafe {
                                    let r = (*self.children[idx]).rect();
                                    r.1
                                }
                            };
                            let bottom_child_end = unsafe {
                                let r = (*self.children[idx + 1]).rect();
                                r.1 + r.3
                            };
                            
                            let combined_h = (bottom_child_end - top_child_y - self.gap).max(0.0);
                            let min_top = self.min_sizes.get(idx).cloned().unwrap_or(20.0);
                            let min_bottom = self.min_sizes.get(idx + 1).cloned().unwrap_or(20.0);
                            
                            let new_top_h = (py - top_child_y - self.gap / 2.0).clamp(min_top, combined_h - min_bottom);
                            
                            let comb_prop = self.proportions[idx] + self.proportions[idx + 1];
                            if combined_h > 0.1 {
                                self.proportions[idx] = (new_top_h / combined_h) * comb_prop;
                                self.proportions[idx + 1] = comb_prop - self.proportions[idx];
                                let (bx, by, bw, bh) = (self.base.x, self.base.y, self.base.w, self.base.h);
                                self.set_rect(bx, by, bw, bh);
                                self.mark_dirty(ctx);
                                handled = true;
                            }
                        }
                    }
                }
            }
        }

        // Hover checking
        let mut new_hovered = None;
        if self.dragging_idx.is_none() {
            let splitters = self.get_splitter_rects();
            for (i, &(sx, sy, sw, sh)) in splitters.iter().enumerate() {
                if px >= sx && px <= sx + sw && py >= sy && py <= sy + sh {
                    new_hovered = Some(i);
                    break;
                }
            }
        }
        if self.hovered_idx != new_hovered {
            self.hovered_idx = new_hovered;
            self.mark_dirty(ctx);
            handled = true;
        }

        handled
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Left {
            match state {
                ElementState::Pressed => {
                    let splitters = self.get_splitter_rects();
                    for (i, &(sx, sy, sw, sh)) in splitters.iter().enumerate() {
                        if px >= sx && px <= sx + sw && py >= sy && py <= sy + sh {
                            self.dragging_idx = Some(i);
                            ctx.set_focused_ptr(self.as_ptr_mut());
                            self.mark_dirty(ctx);
                            return true;
                        }
                    }
                }
                ElementState::Released => {
                    if self.dragging_idx.is_some() {
                        self.dragging_idx = None;
                        ctx.clear_focus();
                        self.mark_dirty(ctx);
                        return true;
                    }
                }
            }
        }

        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let splitters = self.get_splitter_rects();
        for (i, &(sx, sy, sw, sh)) in splitters.iter().enumerate() {
            let is_dragging = self.dragging_idx == Some(i);
            let is_hovered = self.hovered_idx == Some(i);
            if is_dragging {
                quads.push((sx + sw / 2.0 - 1.0, sy, 2.0, sh, [0.36, 0.56, 0.38, 0.8]));
            } else if is_hovered {
                quads.push((sx + sw / 2.0 - 1.0, sy, 2.0, sh, [0.25, 0.25, 0.32, 0.6]));
            } else {
                quads.push((sx + sw / 2.0 - 0.5, sy, 1.0, sh, [0.15, 0.15, 0.18, 0.4]));
            }
        }
        quads
    }

    // SplitBox is a real layout container: aggregate each child's quads and text
    // (clipped to our bounds), so a self-rendering widget placed inside the split
    // is drawn through the widget tree rather than needing to be rendered manually.
    // Mirrors Backplate's aggregation. A child's own background is emitted through
    // exactly one path (rounded → all_rounded_quads, otherwise extra_quads), so the
    // has_rounded guard below prevents a square underlay from doubling a rounded bg.
    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible() {
            return Vec::new();
        }
        let mut quads = self.extra_quads();
        let (wx, wy, ww, wh) = self.rect();
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            let (cx, cy, cw, ch) = widget.rect();
            let child_has_rounded = widget.rounded_corners() != (false, false, false, false);
            for (qx, qy, qw, qh, qc) in widget.all_quads(ctx) {
                if child_has_rounded
                    && (qx - cx).abs() < 0.1 && (qy - cy).abs() < 0.1
                    && (qw - cw).abs() < 0.1 && (qh - ch).abs() < 0.1
                {
                    continue;
                }
                let x0 = qx.max(wx);
                let y0 = qy.max(wy);
                let x1 = (qx + qw).min(wx + ww);
                let y1 = (qy + qh).min(wy + wh);
                if x1 > x0 && y1 > y0 {
                    quads.push((x0, y0, x1 - x0, y1 - y0, qc));
                }
            }
        }
        quads
    }

    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible() {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let (wx, wy, ww, wh) = self.rect();
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            for (qx, qy, qw, qh, qr, qc, corners) in widget.all_rounded_quads(ctx) {
                let x0 = qx.max(wx);
                let y0 = qy.max(wy);
                let x1 = (qx + qw).min(wx + ww);
                let y1 = (qy + qh).min(wy + wh);
                if x1 > x0 && y1 > y0 {
                    quads.push((x0, y0, x1 - x0, y1 - y0, qr, qc, corners));
                }
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            labels.extend(widget.text_labels());
        }
        labels
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let mut result = Vec::new();
        let (wx, wy, ww, wh) = self.rect();
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            for (label, bounds) in widget.text_labels_with_bounds(ctx) {
                let cb = match bounds {
                    Some(b) => {
                        let cx0 = b[0].max(wx);
                        let cy0 = b[1].max(wy);
                        let cx1 = b[2].min(wx + ww);
                        let cy1 = b[3].min(wy + wh);
                        if cx1 > cx0 && cy1 > cy0 {
                            Some([cx0, cy0, cx1, cy1])
                        } else {
                            continue;
                        }
                    }
                    None => Some([wx, wy, wx + ww, wy + wh]),
                };
                result.push((label, cb));
            }
        }
        result
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut result = Vec::new();
        let (wx, wy, ww, wh) = self.rect();
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            for (label, font, bounds) in widget.text_labels_with_font_and_bounds(ctx) {
                let cb = match bounds {
                    Some(b) => {
                        let cx0 = b[0].max(wx);
                        let cy0 = b[1].max(wy);
                        let cx1 = b[2].min(wx + ww);
                        let cy1 = b[3].min(wy + wh);
                        if cx1 > cx0 && cy1 > cy0 {
                            Some([cx0, cy0, cx1, cy1])
                        } else {
                            continue;
                        }
                    }
                    None => Some([wx, wy, wx + ww, wy + wh]),
                };
                result.push((label, font, cb));
            }
        }
        result
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        let mut result = Vec::new();
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            result.extend(widget.get_text_items());
        }
        result
    }

    fn set_hovered(&mut self, v: bool) {
        if !v {
            self.hovered_idx = None;
        }
        for &child in &self.children {
            unsafe {
                (*child).set_hovered(v);
            }
        }
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), _ctx: &mut UiContext) {
        self.children.push(child);
        self.proportions.push(1.0);
        self.min_sizes.push(20.0);
    }

    fn clear_children(&mut self, _ctx: &mut UiContext) {
        self.children.clear();
        self.proportions.clear();
        self.min_sizes.clear();
        self.dragging_idx = None;
        self.hovered_idx = None;
    }
}

unsafe impl Send for SplitBox {}
unsafe impl Sync for SplitBox {}
