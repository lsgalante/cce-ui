//! The traits a host drives a widget through, where a widget offers more than its events: page
//! selection, menus, graphs, spreadsheets, paths, parameters and geometry.

use super::*;

pub trait PageSelector {
    fn selected_page(&self) -> usize;
    fn set_selected_page(&mut self, page: usize);
    fn sidebar_w(&self) -> f32;
}

pub trait MenuController {
    fn menu_click(&mut self) -> Option<(usize, usize)>;
    fn trigger_menu_click(&mut self, menu_idx: usize, item_idx: usize);
    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool);
    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]);
    fn is_menu_bar(&self) -> bool;
    fn is_menu_open(&self) -> bool;
    fn menu_items(&self) -> Vec<String>;
    fn menu_item_checked(&self) -> Vec<Option<bool>>;
    fn is_vertical(&self) -> bool;
    fn menu_names(&self) -> Vec<String>;
    fn menu_items_list(&self) -> Vec<Vec<String>>;
    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>>;
    fn take_context_change(&mut self) -> Option<usize>;
    fn set_context_selected(&mut self, selected: usize);
    fn set_center_items(&mut self, center: bool);
    fn get_menu_items_at(&self, px: f32, py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)>;
}

pub trait GraphController {
    fn set_nodes(&mut self, nodes: &[GraphNode]);
    fn get_nodes(&self) -> Vec<GraphNode>;
    fn selected_node(&self) -> Option<usize>;
    fn set_selected_node(&mut self, idx: Option<usize>);
    fn double_clicked_node(&self) -> Option<usize>;
    fn clear_double_clicked_node(&mut self);
    fn set_grid_snap_enabled(&mut self, enabled: bool);
    fn take_node_geom_toggle(&mut self) -> Option<(usize, bool)>;
    fn set_grid_snap(&mut self, gx: f32, gy: f32);
    /// The grid's ONE size per axis: the pitch, from the centre of one grid
    /// line to the centre of the next. Nodes are centred on the lattice
    /// intersections. Leaves the node size alone.
    fn set_grid_pitch(&mut self, px: f32, py: f32);
    /// The node body's size, independent of the pitch — hosts scale it with
    /// their zoom as they scale the pitch.
    fn set_node_size(&mut self, w: f32, h: f32);
    /// The older cell-and-gap description of the same lattice — a cell plus
    /// its gap is a pitch, and the node body is the cell. Kept for hosts
    /// that still speak it (cce-files, cce-graph); new code sets the pitch.
    fn set_grid_sizes(&mut self, gx: f32, gy: f32);
    /// The gap half of the cell-and-gap description; see [`set_grid_sizes`].
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32);
    /// The lattice intersection node (0, 0) is centred on, window-absolute.
    fn set_grid_origin(&mut self, ox: f32, oy: f32);
    fn grid_origin(&self) -> (f32, f32);
    fn set_show_network_grid(&mut self, show: bool);
    fn take_pending_connection(&mut self) -> Option<(String, String)>;
    /// [`Self::take_pending_connection`] with the INPUT PORT the connection
    /// was dropped on: (input node id, output node name, port). A host whose
    /// nodes read several wires (the k-th `node` parameter into port k)
    /// writes the one the port is. Taking either takes the connection.
    fn take_pending_connection_to_port(&mut self) -> Option<(String, String, usize)> {
        self.take_pending_connection().map(|(id, name)| (id, name, 0))
    }
    /// A node dropped onto a wire, to be spliced in between its ends:
    /// (dragged node id, the wire's upstream node NAME — what Input params
    /// store, the wire's downstream node id). The host rewires both Input
    /// params: dragged.Input = upstream name, downstream.Input = dragged's
    /// name. Default None for hosts whose graphs have no wires to splice.
    fn take_pending_splice(&mut self) -> Option<(String, String, String)> {
        None
    }
    /// A node dropped onto another node, which it swapped places with:
    /// (dragged node id, the other node's id). The widget has traded their
    /// cells; the host trades the rest. Only while the host opted in
    /// (`Graph::set_swap_on_drop`); default None.
    fn take_pending_swap(&mut self) -> Option<(String, String)> {
        None
    }
    /// The wire into an Input that runs through the body a node would have
    /// at lattice cell (col, row), as (upstream id, downstream id): where a
    /// node ADDED there splices in, by the hit test a drop uses. Default
    /// None for hosts whose graphs have no wires to splice.
    fn input_wire_through_cell(&self, _col: f32, _row: f32) -> Option<(String, String)> {
        None
    }
    fn cancel_connecting(&mut self);
    fn is_node_rect(&self, qx: f32, qy: f32, qw: f32, qh: f32) -> bool;
    /// The topmost node whose body contains (px, py), window-absolute coords.
    fn node_at(&self, px: f32, py: f32) -> Option<usize>;
    /// The corner radius of anything node-shaped on the grid at the current
    /// zoom — the cursor, the drop-target highlight (0 = square).
    fn cell_corner_radius(&self) -> f32;
    /// The flat-geometry emission with grid cells tagged by their surviving
    /// rounded corners — for hosts that draw the graph's quads themselves
    /// (the designer) and want cells as superellipse tiles.
    fn geometry_quads_tagged(&self, rect: crate::scene::layout::Rect) -> Vec<TaggedQuad>;
    /// The grid lines and origin axes, flat, over what is beneath — for
    /// hosts that draw the graph's quads themselves, called at the point in
    /// their walk where the grid goes (under the wires and nodes).
    fn paint_grid(&self, rect: crate::scene::layout::Rect, pc: &mut crate::scene::paint::PaintCtx);
    /// The wires and the connection being dragged out, stroked in the wire
    /// style in effect — for hosts that draw the graph's quads themselves,
    /// called after [`paint_grid`](Self::paint_grid) and under the nodes.
    /// The quads carry no wires.
    fn paint_wires(&self, rect: crate::scene::layout::Rect, pc: &mut crate::scene::paint::PaintCtx);
    /// The pixel rect the in-flight node drag will deposit its body on
    /// (`commit_drag`'s resolution), for hosts' drop-target highlight.
    /// None outside a node drag.
    fn drop_target_cell_rect(&self) -> Option<(f32, f32, f32, f32)>;
}

pub trait SpreadsheetController {
    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>);
    /// The table as columns of values ([`SheetColumn`]), a column a header:
    /// the cells are written as they are painted, so a refill costs a copy
    /// of the values. Rows of text, by default.
    fn set_spreadsheet_columns(&mut self, headers: Vec<String>, columns: Vec<SheetColumn>) {
        let n = columns.iter().map(SheetColumn::len).max().unwrap_or(0);
        let rows = (0..n).map(|r| columns.iter().map(|c| c.cell(r)).collect()).collect();
        self.set_spreadsheet_data(headers, rows);
    }
    /// The selected rows, as indices into the rows last set, ascending.
    fn selected_rows(&self) -> Vec<usize> {
        Vec::new()
    }
    /// Replace the selection; a row the table does not have is left out.
    fn set_selected_rows(&mut self, _rows: &[usize]) {}
    /// Whether the selection changed since this was last asked.
    fn take_selection_change(&mut self) -> bool {
        false
    }
}

pub trait PathController {
    fn set_path(&mut self, segments: &[String]);
    fn path_click(&mut self) -> Option<usize>;
}

pub trait ParamController {
    fn node_params(&self) -> Vec<(String, String, String)>;
    fn set_display_params(&mut self, params: &[(String, String, String)]);
}

pub trait GeomController {
    fn set_geom_visible(&mut self, visible: bool);
    fn geom_visible(&self) -> bool;
    fn take_geom_toggle(&mut self) -> bool;
}
