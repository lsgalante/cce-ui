//! The text box's tests, by topic.
//!
//! | module | covers |
//! |---|---|
//! | `shaping` | glyph offsets, right-to-left text, wrapping by shaped width |
//! | `pointer` | focus, clicks, drags, selection and its highlight, the context menu |
//! | `editing` | composition, undo runs, the clipboard and a password box, the search box's clear |
//! | `geometry` | horizontal scrolling, the border, where a tall box's text starts |

mod editing;
mod geometry;
mod pointer;
mod shaping;

use super::*;
