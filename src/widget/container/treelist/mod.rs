//! `TreeList`: a JSON value shown as an expandable tree of key paths — sections that collapse,
//! leaves with their values (a colour value with its swatch), and annotations — searchable through
//! its own search box, editable through an add-key button with its popover and an inline rename
//! editor (all `widget::Embedded` children, in the context while the tree is). A double click
//! renames, a right click offers copy, delete and expand / collapse. It scrolls in a `ScrollBox`
//! whose bar is the DE's centred, sink-behind one (`docs/widgets.md`, "Scrollbars").
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the tree element types, the struct, construction, the embedded fields and their placement, rebuilding, selection, `impl Layout` |
//! | `tree` | key paths, the search match, building rows from the value; copy, delete, expand and collapse |
//! | `paint` | the well, the fields, chevrons and labels, `impl Paint` |
//! | `input` | the mouse, motion and key handlers, `impl Input` |

mod input;
mod paint;
mod tree;
#[cfg(test)]
mod tests;

use tree::*;

use crate::widget::*;
use crate::l10n::tr;
use crate::widget::container::scroll_box::ScrollBox;
use crate::widget::display::TextLabel;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use std::collections::HashSet;


/// The add-key button's label, which also sizes it.
const ADD_KEY_LABEL: &str = "+ Add Key";

#[derive(Debug, Clone, PartialEq)]
pub enum TreeElement {
    Section {
        path: String,
        name: String,
        indent: usize,
        collapsed: bool,
    },
    Leaf {
        path: String,
        name: String,
        indent: usize,
        val: serde_json::Value,
        original_idx: usize,
    }
}

/// The fields' places (`TreeList::field_rects`).
struct FieldRects {
    search_box: Rect,
    add_key_btn: Rect,
    popover_box: Rect,
}

#[derive(Debug)]
pub struct TreeList {
    pub base: Widget,
    pub scroll_box: ScrollBox,
    /// The search field: an [`Embedded`] child, the context's once the tree is (see
    /// `register_embedded_children`).
    pub search_box: Embedded<Adapted<TextBox>>,
    /// What the search field held when the rows were last built (`rebuild_tree` has no
    /// context to read the field through).
    query: String,
    pub add_key_btn: Embedded<Adapted<Button>>,
    pub add_key_popover_open: bool,
    pub add_key_popover_box: Embedded<Adapted<TextBox>>,
    pub new_key_path_request: Option<String>,
    pub flat_keys: Vec<(String, serde_json::Value)>,
    pub annotations: Vec<Option<String>>,
    pub collapsed_sections: HashSet<String>,
    pub items: Vec<TreeElement>,
    pub selected_key_idx: Option<usize>,
    pub hovered_row_idx: Option<usize>,
    pub item_height: f32,
    pub clicked_item: Option<TreeElement>,
    pub right_clicked_section: Option<String>,
    pub last_scroll_y: f32,
    /// Lights the recess rim (`paint` has no UiContext, so this mirrors focus).
    /// Driven by FocusIn/FocusOut by default; an app whose inline editors float
    /// over the tree (cce-data-editor) overrides it per input event with its own
    /// focus-within computation — those editors take ctx focus away from the
    /// tree while still being, visually, part of the tree pane.
    pub focused: bool,
    pub deleted_key_path: Option<String>,
    /// The inline rename editor: in the context only while a rename is under way.
    pub edit_box: Embedded<Adapted<TextBox>>,
    pub editing_key_idx: Option<usize>,
    pub double_click_timer: Option<(web_time::Instant, usize)>,
    pub rename_request: Option<(String, String)>,
}

impl TreeList {
    pub fn new() -> Adapted<TreeList> {
        let mut scroll_box = ScrollBox::new();
        scroll_box.show_background = false;
        // The DE's scrollbar: down the tree's centre line, behind its plate
        // until a scroll raises it (`paint`).
        scroll_box.sink_behind = true;
        Adapted::new(TreeList {
            base: Widget::new(),
            scroll_box,
            search_box: Embedded::new(TextBox::new(String::new()).with_search().with_update_on_type(true)),
            query: String::new(),
            add_key_btn: Embedded::new(Button::new(0.0, 0.0, 80.0, 26.0).with_label(ADD_KEY_LABEL)),
            add_key_popover_open: false,
            add_key_popover_box: Embedded::new(TextBox::new(String::new()).with_placeholder("new.key.path").with_multiline(false)),
            new_key_path_request: None,
            flat_keys: Vec::new(),
            annotations: Vec::new(),
            collapsed_sections: HashSet::new(),
            items: Vec::new(),
            selected_key_idx: None,
            hovered_row_idx: None,
            item_height: 28.0,
            clicked_item: None,
            right_clicked_section: None,
            last_scroll_y: 0.0,
            focused: false,
            deleted_key_path: None,
            edit_box: Embedded::new(TextBox::new(String::new()).with_multiline(false).with_draw_bg_border(true)),
            editing_key_idx: None,
            double_click_timer: None,
            rename_request: None,
        })
    }

    pub fn take_new_key_path_request(&mut self) -> Option<String> {
        self.new_key_path_request.take()
    }

    pub fn popover_rect_geom(&self) -> (f32, f32, f32, f32) {
        let b = self.field_rects().add_key_btn;
        let (bx, by, bw, bh) = (b.x, b.y, b.width, b.height);
        let popover_w = 220.0;
        let popover_h = 36.0;
        let popover_x = bx + bw - popover_w;
        let popover_y = by + bh + 4.0;
        (popover_x, popover_y, popover_w, popover_h)
    }

    /// Where the search field, the add-key button and the add-key popover's box stand, from
    /// the tree's content rect: the field across the top less the button at its right end,
    /// the popover's box inside the popover under the button.
    fn field_rects(&self) -> FieldRects {
        let (x, y, w) = (self.base.x, self.base.y, self.base.w);
        let search_margin_x = 8.0;
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let (btn_family, btn_size) = crate::layout::parse_font_string(&crate::layout::button_font());
        let label_w = crate::widget::display::measure_text_width(ADD_KEY_LABEL, &btn_family, btn_size.unwrap_or(12.0));
        let button_width = label_w + 2.0 * crate::layout::button_padding();
        let button_x = x + w - search_margin_x - button_width;
        let add_key_btn = Rect { x: button_x, y: y + search_margin_y, width: button_width, height: search_h };
        let popover_w = 220.0;
        let popover_h = 36.0;
        let (px, py) = (button_x + button_width - popover_w, add_key_btn.y + search_h + 4.0);
        FieldRects {
            search_box: Rect { x: x + search_margin_x, y: y + search_margin_y, width: w - 2.0 * search_margin_x - button_width - 6.0, height: search_h },
            add_key_btn,
            popover_box: Rect { x: px + 8.0, y: py + 5.0, width: popover_w - 16.0, height: popover_h - 10.0 },
        }
    }

    /// Place the fields the tree still holds by value (it is not in a context).
    fn place_held_fields(&mut self) {
        let r = self.field_rects();
        for (field, rect) in [(&mut self.search_box, r.search_box), (&mut self.add_key_popover_box, r.popover_box)] {
            if let Some(f) = field.here_mut() {
                f.set_rect(rect.x, rect.y, rect.width, rect.height);
            }
        }
        if let Some(b) = self.add_key_btn.here_mut() {
            b.set_rect(r.add_key_btn.x, r.add_key_btn.y, r.add_key_btn.width, r.add_key_btn.height);
        }
    }

    pub fn focus_search(&mut self, ctx: &mut UiContext) {
        self.search_box.attach(ctx);
        ctx.set_focused_id(self.search_box.id());
        self.search_box.get_mut(ctx).focus();
    }

    pub fn set_flat_keys(&mut self, flat_keys: Vec<(String, serde_json::Value)>) {
        self.flat_keys = flat_keys;
        self.rebuild_tree();
    }

    pub fn rebuild_tree(&mut self) {
        self.items = build_tree(&self.flat_keys, &self.annotations, &self.collapsed_sections, &self.query);
        let content_h = self.items.len() as f32 * self.item_height;
        let h = self.base.h;
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        let header_h = 26.0;
        self.scroll_box.update_bounds(content_h, self.scroll_box.viewport_y, h - offset_y - header_h);
    }

    /// The on-screen rect of a leaf row, or `None` unless the row is FULLY
    /// visible. Deliberately full-containment, unlike the draw-side
    /// virtualization (which returns partial rows to be drawn cut by the
    /// clip): this positions a floating overlay (cce-data-editor's inline
    /// value editors) that draws OVER the well unclipped, and an editor
    /// hanging half off the list edge is worse than one that waits for its
    /// row to scroll fully into view.
    pub fn get_row_rect(&self, original_idx: usize) -> Option<(f32, f32, f32, f32)> {
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;
        
        let row_idx = self.items.iter().position(|item| {
            match item {
                TreeElement::Leaf { original_idx: idx, .. } => *idx == original_idx,
                _ => false,
            }
        })?;

        let row_y = list_top + row_idx as f32 * self.item_height - self.scroll_box.scroll_y;
        if row_y >= list_top && row_y + self.item_height <= list_bottom {
            Some((self.scroll_box.base.x, row_y, self.scroll_box.base.w, self.item_height))
        } else {
            None
        }
    }

    pub fn take_clicked_item(&mut self) -> Option<TreeElement> {
        self.clicked_item.take()
    }

    pub fn take_deleted_key_path(&mut self) -> Option<String> {
        self.deleted_key_path.take()
    }

    pub fn take_rename_request(&mut self) -> Option<(String, String)> {
        self.rename_request.take()
    }


    pub fn scroll_to_selected_key(&mut self) {
        if let Some(selected_idx) = self.selected_key_idx {
            let visible_row_idx = self.items.iter().position(|item| {
                if let TreeElement::Leaf { original_idx, .. } = item {
                    *original_idx == selected_idx
                } else {
                    false
                }
            });
            if let Some(row_idx) = visible_row_idx {
                let row_top = row_idx as f32 * self.item_height;
                let row_bottom = row_top + self.item_height;
                let viewport_h = self.scroll_box.viewport_h;
                
                if row_top < self.scroll_box.scroll_y {
                    self.scroll_box.scroll_y = row_top;
                } else if row_bottom > self.scroll_box.scroll_y + viewport_h {
                    self.scroll_box.scroll_y = (row_bottom - viewport_h).max(0.0);
                }
                
                let max_scroll = (self.scroll_box.content_h - viewport_h).max(0.0);
                self.scroll_box.scroll_y = self.scroll_box.scroll_y.clamp(0.0, max_scroll);
            }
        }
    }

    pub fn select_and_show_key(&mut self, key_path: &str) -> bool {
        let found_idx = self.flat_keys.iter().position(|(k, _)| k == key_path);
        if let Some(idx) = found_idx {
            self.selected_key_idx = Some(idx);
            
            let parts: Vec<&str> = key_path.split('.').collect();
            let mut current = String::new();
            let mut expanded_any = false;
            for i in 0..parts.len() - 1 {
                if !current.is_empty() {
                    current.push('.');
                }
                current.push_str(parts[i]);
                if self.collapsed_sections.contains(&current) {
                    self.collapsed_sections.remove(&current);
                    expanded_any = true;
                }
            }
            if expanded_any {
                self.rebuild_tree();
            }
            
            self.scroll_to_selected_key();
            true
        } else {
            false
        }
    }
}

impl Layout for TreeList {
    /// The legacy `set_rect` body: cache the CONTENT rect on the internal base and
    /// arrange the field widgets (search box, add-key button, popover box, scroll box)
    /// in it. The content rect, not `rect_assigned`'s block: the adapter's block holds
    /// the detached label strip above the content, and `paint` draws the well at the
    /// content rect — fields placed from the block sat one strip above the well, the
    /// search box straddling its top edge over the label and the header row's text
    /// clipped away outside its bounds (the gallery's labelled tree).
    fn arrange_children(&mut self, rect: Rect) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;

        // The fields are placed here while the tree holds them; once they are the context's,
        // `register_embedded_children` places them (it has the context, `set_rect` does not).
        self.place_held_fields();

        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        let header_h = 26.0;
        self.scroll_box.set_rect(x, y + offset_y + header_h, w, h - offset_y - header_h);

        let content_h = self.items.len() as f32 * self.item_height;
        self.scroll_box.update_bounds(content_h, y + offset_y + header_h, h - offset_y - header_h);
        self.last_scroll_y = self.scroll_box.scroll_y;
    }

    /// Keep the field widgets registered/linked under the adapter every tick (the legacy
    /// `set_parent` side effect; also heals the inline rename editor's registry entry).
    fn register_embedded_children(&mut self, host_id: WidgetId, ctx: &mut UiContext) {
        // In the context but deliberately NOT tree-linked (6bd): the tree is a SELF-ROUTING
        // composite — mouse_body/move_body/key_body forward to every field widget
        // internally, so the router's children-first descent double-delivered AND starved
        // the tree-level logic (the recorded 6as latents: the hit add-key button consumed
        // the press before mouse_body's take_click toggle ran, so the popover never
        // opened, and the wheel died the same way). Being the context's keeps the ids
        // resolvable for focus, coverage, the spatial grid and the accessibility tree.
        // (The rename editor is linked while it is up; see `mouse_body`.)
        let _ = host_id;
        self.search_box.attach(ctx);
        self.add_key_btn.attach(ctx);
        self.add_key_popover_box.attach(ctx);
        let r = self.field_rects();
        self.search_box.get_mut(ctx).set_rect(r.search_box.x, r.search_box.y, r.search_box.width, r.search_box.height);
        self.add_key_btn.get_mut(ctx).set_rect(r.add_key_btn.x, r.add_key_btn.y, r.add_key_btn.width, r.add_key_btn.height);
        self.add_key_popover_box.get_mut(ctx).set_rect(r.popover_box.x, r.popover_box.y, r.popover_box.width, r.popover_box.height);
        // The popover's box is shown only while the popover is: hidden, it is no Tab stop
        // and no field a screen reader finds, where it stood in both, drawn or not.
        let open = self.add_key_popover_open;
        let b = self.add_key_popover_box.get_mut(ctx);
        if WidgetHost::visible(b) != open {
            WidgetHost::set_visible(b, open);
        }
    }

    fn release_embedded_children(&mut self, ctx: &mut UiContext) {
        self.search_box.detach(ctx);
        self.add_key_btn.detach(ctx);
        self.add_key_popover_box.detach(ctx);
        self.edit_box.detach(ctx);
    }
}

unsafe impl Send for TreeList {}

unsafe impl Sync for TreeList {}
