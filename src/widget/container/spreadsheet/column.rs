//! [`SheetColumn`]: one column of the table as its values — its cells written on demand, its
//! widest cell worked out from the values, and two rows compared by value.

use super::*;

/// One column of a [`Spreadsheet`], as its values: the widget writes the
/// cells it paints, so a column of a hundred thousand numbers costs a copy
/// of them, not a hundred thousand strings.
#[derive(Debug, Clone, PartialEq)]
pub enum SheetColumn {
    /// Cells as given. Sorted numerically where both cells parse as numbers,
    /// as text otherwise.
    Text(Vec<String>),
    /// Whole numbers.
    Int(Vec<i64>),
    /// Numbers written to `decimals` places (`format!("{:.*}")`).
    Float { values: Vec<f32>, decimals: usize },
}

impl SheetColumn {
    /// How many cells the column has.
    pub fn len(&self) -> usize {
        match self {
            SheetColumn::Text(v) => v.len(),
            SheetColumn::Int(v) => v.len(),
            SheetColumn::Float { values, .. } => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Row `row`'s cell as it is shown; empty past the column's end.
    pub fn cell(&self, row: usize) -> String {
        match self {
            SheetColumn::Text(v) => v.get(row).cloned().unwrap_or_default(),
            SheetColumn::Int(v) => v.get(row).map(|n| n.to_string()).unwrap_or_default(),
            SheetColumn::Float { values, decimals } => {
                values.get(row).map(|x| format!("{:.*}", decimals, x)).unwrap_or_default()
            }
        }
    }

    /// The widest cell, in characters, as [`Self::cell`] writes it — worked
    /// out from the values without writing them: a whole number's widest is
    /// its least or its greatest, a float's too (the sign and the integer
    /// digits are what vary; the decimals are fixed), plus `NaN` / `inf` /
    /// `-inf` where those occur. 0 for an empty column.
    pub fn max_chars(&self) -> usize {
        match self {
            SheetColumn::Text(v) => v.iter().map(|c| c.chars().count()).max().unwrap_or(0),
            SheetColumn::Int(v) => {
                let Some(&first) = v.first() else { return 0 };
                let (lo, hi) = v.iter().fold((first, first), |(lo, hi), &x| (lo.min(x), hi.max(x)));
                lo.to_string().len().max(hi.to_string().len())
            }
            SheetColumn::Float { values, decimals } => {
                // Plain comparisons, which the compiler vectorizes, where
                // `f32::min` / `max` carry NaN handling a finite value does
                // not need: at a million values a refill (the designer's
                // table at 57k points, every frame of a playback) this pass
                // was 1.35 ms (until 2026-10-07). A tie between 0 and -0
                // takes -0, whose `-` is a character wider.
                // No branch in the loop: a value that is not finite stands
                // out of the range it does not belong to, and what is
                // special is OR'd up and settled after.
                let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
                let (mut neg_zero, mut nan_or_inf, mut neg_inf) = (false, false, false);
                for &x in values {
                    let finite = x.is_finite();
                    let a = if finite { x } else { f32::INFINITY };
                    let b = if finite { x } else { f32::NEG_INFINITY };
                    lo = if a < lo { a } else { lo };
                    hi = if b > hi { b } else { hi };
                    neg_zero |= x == 0.0 && x.is_sign_negative();
                    nan_or_inf |= !finite && x != f32::NEG_INFINITY;
                    neg_inf |= x == f32::NEG_INFINITY;
                }
                if lo == 0.0 && neg_zero {
                    lo = -0.0;
                }
                let odd = if neg_inf { 4 } else if nan_or_inf { 3 } else { 0 };
                let w = |x: f32| format!("{:.*}", decimals, x).len();
                let finite = if lo <= hi { w(lo).max(w(hi)) } else { 0 };
                finite.max(odd)
            }
        }
    }

    /// The order of rows `a` and `b` by this column: by value for numbers
    /// (a NaN is equal to anything, so it keeps its place), by
    /// `Spreadsheet::cmp_cells` for text.
    pub(super) fn cmp_rows(&self, a: usize, b: usize) -> std::cmp::Ordering {
        use std::cmp::Ordering::Equal;
        match self {
            SheetColumn::Text(v) => Spreadsheet::cmp_cells(
                v.get(a).map(String::as_str).unwrap_or(""),
                v.get(b).map(String::as_str).unwrap_or(""),
            ),
            SheetColumn::Int(v) => v.get(a).cmp(&v.get(b)),
            SheetColumn::Float { values, .. } => match (values.get(a), values.get(b)) {
                (Some(x), Some(y)) => x.partial_cmp(y).unwrap_or(Equal),
                (x, y) => x.is_some().cmp(&y.is_some()),
            },
        }
    }
}
