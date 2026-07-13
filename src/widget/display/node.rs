//! Narrow-trait `Node` (Phase 5k) — a network-editor node box: draggable with grid snap
//! (self-moving, via [`Input::drag_reposition`]), a geometry-visibility toggle sub-zone, and
//! two controller capabilities ([`ParamController`] + [`GeomController`]) re-exposed through
//! the `Input` hooks for the legacy `WidgetHost::as_*_controller` downcasts.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, GeomController, Input, Layout, MouseButton, Paint,
    ParamController,
};

#[derive(Debug, Clone)]
pub struct Node {
    hovered: bool,
    selected: bool,
    dragging: bool,
    drag_ox: f32,
    drag_oy: f32,
    bounds: Option<(f32, f32, f32, f32)>,
    grid_snap_x: f32,
    grid_snap_y: f32,
    grid_origin_x: f32,
    grid_origin_y: f32,
    name: String,
    pub parameters: Vec<(String, String, String)>,
    geom_visible: bool,
    geom_toggled: bool,
    pub(crate) toggle_hovered: bool,
}

impl Node {
    pub fn new(x: f32, y: f32, w: f32, h: f32, name: &str) -> Adapted<Node> {
        let mut node = Adapted::new(Node {
            hovered: false,
            selected: false,
            dragging: false,
            drag_ox: 0.0,
            drag_oy: 0.0,
            bounds: None,
            grid_snap_x: 0.0,
            grid_snap_y: 0.0,
            grid_origin_x: 0.0,
            grid_origin_y: 0.0,
            name: name.to_string(),
            parameters: Vec::new(),
            geom_visible: true,
            geom_toggled: false,
            toggle_hovered: false,
        });
        crate::widget::WidgetHost::set_rect(&mut node, x, y, w, h);
        node
    }

    pub(crate) fn toggle_rect(rect: Rect) -> (f32, f32, f32, f32) {
        (rect.x + rect.width - 30.0, rect.y + (rect.height - 18.0) / 2.0, 18.0, 18.0)
    }

    fn in_toggle(rect: Rect, px: f32, py: f32) -> bool {
        let (tx, ty, tw, th) = Self::toggle_rect(rect);
        px >= tx && px < tx + tw && py >= ty && py < ty + th
    }

    pub fn set_grid_snap(&mut self, gx: f32, gy: f32) {
        self.grid_snap_x = gx;
        self.grid_snap_y = gy;
    }
    pub fn set_grid_origin(&mut self, ox: f32, oy: f32) {
        self.grid_origin_x = ox;
        self.grid_origin_y = oy;
    }
    pub fn set_node_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
}

impl Adapted<Node> {
    pub fn with_params(mut self, params: &[(&str, &str)]) -> Self {
        self.parameters =
            params.iter().map(|(k, v)| (k.to_string(), v.to_string(), "string".to_string())).collect();
        self
    }

    pub fn with_grid_snap(mut self, gx: f32, gy: f32) -> Self {
        self.set_grid_snap(gx, gy);
        self
    }
}

impl Layout for Node {}

impl Paint for Node {
    fn color(&self) -> [f32; 4] {
        if self.dragging {
            colors::node_drag_color()
        } else if self.selected {
            colors::node_selected_color()
        } else {
            colors::node_color()
        }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        ctx.quad(rect, Paint::color(self));

        let (tx, ty, tw, th) = Self::toggle_rect(rect);
        let bg_color = if self.toggle_hovered { colors::TOGGLE_HOVER } else { colors::TOGGLE_OFF };
        ctx.quad(Rect { x: tx, y: ty, width: tw, height: th }, bg_color);
        if self.geom_visible {
            let inset = 3.0;
            ctx.quad(
                Rect { x: tx + inset, y: ty + inset, width: tw - inset * 2.0, height: th - inset * 2.0 },
                colors::TOGGLE_ON,
            );
        }

        ctx.text(
            self.name.clone(),
            rect.x + rect.width + 8.0,
            crate::layout::align_text_y(rect.y, rect.height, 14.0, 0.0),
            14.0,
            [0xcc, 0xcc, 0xd4],
        );
    }
}

impl Input for Node {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let r = ectx.rect;
                let was_hovered = self.hovered;
                self.hovered =
                    *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height;
                let was_toggle = self.toggle_hovered;
                self.toggle_hovered = Self::in_toggle(r, *px, *py);
                was_hovered != self.hovered || was_toggle != self.toggle_hovered
            }
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                if Self::in_toggle(ectx.rect, *px, *py) {
                    self.geom_visible = !self.geom_visible;
                    self.geom_toggled = true;
                } else {
                    self.drag_begin(*px, *py, ectx.rect);
                }
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, .. } => {
                if self.dragging {
                    self.drag_end();
                    true
                } else {
                    false
                }
            }
            // Legacy `focus()`/`unfocus()` toggled selection; hosts reach them through the
            // adapter's focus forwards, which arrive here as focus events.
            Event::FocusIn => {
                self.selected = true;
                true
            }
            Event::FocusOut => {
                self.selected = false;
                true
            }
            _ => false,
        }
    }

    fn draggable(&self, _rect: Rect) -> bool {
        !self.toggle_hovered
    }
    fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn drag_begin(&mut self, px: f32, py: f32, rect: Rect) {
        self.dragging = true;
        self.drag_ox = px - rect.x;
        self.drag_oy = py - rect.y;
    }

    fn drag_reposition(&mut self, px: f32, py: f32, rect: Rect) -> Option<(f32, f32)> {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        let (nx, ny) = if let Some((bx, by, bw, bh)) = self.bounds {
            (nx.clamp(bx, bx + bw - rect.width), ny.clamp(by, by + bh - rect.height))
        } else {
            (nx, ny)
        };
        let nx = if self.grid_snap_x > 0.0 {
            let relative = nx - self.grid_origin_x;
            (relative / self.grid_snap_x).round() * self.grid_snap_x + self.grid_origin_x
        } else {
            nx
        };
        let ny = if self.grid_snap_y > 0.0 {
            let relative = ny - self.grid_origin_y;
            (relative / self.grid_snap_y).round() * self.grid_snap_y + self.grid_origin_y
        } else {
            ny
        };
        if (nx - rect.x).abs() > 0.01 || (ny - rect.y).abs() > 0.01 {
            Some((nx, ny))
        } else {
            None
        }
    }

    fn drag_end(&mut self) {
        self.dragging = false;
    }

    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }

}

impl ParamController for Node {
    fn node_params(&self) -> Vec<(String, String, String)> {
        self.parameters.clone()
    }
    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        self.parameters = params.to_vec();
    }
}

impl GeomController for Node {
    fn set_geom_visible(&mut self, visible: bool) {
        self.geom_visible = visible;
    }
    fn geom_visible(&self) -> bool {
        self.geom_visible
    }
    fn take_geom_toggle(&mut self) -> bool {
        std::mem::take(&mut self.geom_toggled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::WidgetHost;

    #[test]
    fn toggle_click_flips_geom_and_press_starts_drag() {
        let mut ctx = UiContext::new();
        let mut node = Node::new(100.0, 100.0, 120.0, 40.0, "geo1");
        let (id, ptr) = (node.id(), node.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Toggle zone: (100+120-30, 100+11) => 18x18 at (190, 111).
        assert!(node.mouse_input(MouseButton::Left, ElementState::Pressed, 195.0, 115.0, &mut ctx));
        let geom: &mut dyn GeomController = &mut *node;
        assert!(!geom.geom_visible(), "toggle click hides geometry");
        assert!(geom.take_geom_toggle(), "toggle flag set once");
        assert!(!geom.take_geom_toggle(), "…and drained");

        // A press outside the toggle starts a drag; reposition snaps to the drag origin.
        assert!(node.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 110.0, &mut ctx));
        assert!(node.is_dragging());
        assert!(node.drag_update(150.0, 130.0));
        assert_eq!(WidgetHost::rect(&node), (140.0, 120.0, 120.0, 40.0), "moved by the pointer delta");
        assert!(node.mouse_input(MouseButton::Left, ElementState::Released, 150.0, 130.0, &mut ctx));
        assert!(!node.is_dragging());
    }

    #[test]
    fn param_controller_roundtrips_through_element() {
        let mut node = Node::new(0.0, 0.0, 10.0, 10.0, "n").with_params(&[("k", "v")]);
        let params = ParamController::node_params(&*node);
        assert_eq!(params, vec![("k".to_string(), "v".to_string(), "string".to_string())]);
        ParamController::set_display_params(&mut *node, &[("a".to_string(), "b".to_string(), "int".to_string())]);
        assert_eq!(node.parameters.len(), 1);
        assert_eq!(node.parameters[0].2, "int");
    }
}
