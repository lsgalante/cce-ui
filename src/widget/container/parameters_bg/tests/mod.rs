//! The parameter pane's tests, by topic. `panel_with` builds a pane over a list of rows.
//!
//! | module | covers |
//! |---|---|
//! | `rows` | what each row type builds and draws: pickers, glyphs, fields, labels inline or stacked, separators, vector groups and the trackball, ramps, toggles |
//! | `code` | the inline code editor: when edits apply, indenting, selection and undo, the error line, context actions |
//! | `sections` | collapsing sections, their outline, and the gaps between rows and sections |
//! | `pointer` | scrolling, hover, and which control a wheel or finger gesture belongs to |

mod code;
mod pointer;
mod rows;
mod sections;

use super::*;
use super::code::get_cursor_line_col;
use crate::context::UiContext;

fn panel_with(params: &[(&str, &str, &str)]) -> Adapted<ParametersBg> {
    let mut p = ParametersBg::new();
    let params: Vec<(String, String, String)> = params
        .iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect();
    ParamController::set_display_params(&mut *p, &params);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 400.0);
    p
}
