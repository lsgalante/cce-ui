//! Laying out a run of spans: split into words and the pieces between them, each measured,
//! wrapped greedily, styled, and merged with its neighbours of the same style into one prim; inline
//! images flow with the text.

use super::*;

impl<'a> Layouter<'a> {
    /// Lay out inline spans from `(x, y)` wrapping at `x + w`; returns the
    /// y below the last line.
    pub(super) fn inline(&mut self, spans: &[Span], x: f32, w: f32, y: f32, base: RunStyle) -> f32 {
        let lh = self.theme.line_h(base.size);
        let right = x + w;
        // An image embed inside the text is one box in the flow, the size
        // its image fits at: its link text becomes a single placeholder
        // word so the tokenizer cannot split it.
        let boxes: Vec<Option<(f32, f32)>> = spans.iter().map(|s| self.inline_image(s, w)).collect();
        // One with no image yet (loading, missing) shows as its link — by
        // file name when its alias is only a size (`|300`), not "300".
        let sized_link = |s: &Span| matches!(&s.link, Some(SpanLink::Embed { .. })) && cce_vault::markdown::embed_size(&s.text).is_some();
        let owned: Vec<Span>;
        let spans: &[Span] = if boxes.iter().any(Option::is_some) || spans.iter().any(sized_link) {
            owned = spans
                .iter()
                .zip(&boxes)
                .map(|(s, b)| match (&s.link, b) {
                    (_, Some(_)) => Span { text: IMAGE_MARK.to_string(), ..s.clone() },
                    (Some(SpanLink::Embed { target, .. }), None) if sized_link(s) => Span { text: target.clone(), ..s.clone() },
                    _ => s.clone(),
                })
                .collect();
            &owned
        } else {
            spans
        };
        let looks: Vec<Look> = spans.iter().map(|s| self.look(s, base)).collect();
        let spaces: Vec<f32> = looks.iter().map(|l| self.space_width(l)).collect();
        let mut cx = x;
        let mut line_y = y;
        // A space seen since the last placed word, in the style it was in.
        let mut gap: Option<f32> = None;
        let mut run: Option<Run> = None;
        let mut any = false;

        for tok in tokens(spans) {
            match tok {
                Tok::Break => {
                    line_y += self.flush(&mut run, line_y, lh);
                    cx = x;
                    gap = None;
                }
                Tok::Space(si) => {
                    if cx > x {
                        gap = Some(spaces[si]);
                    }
                }
                Tok::Word(segs) => {
                    any = true;
                    let widths: Vec<f32> = segs
                        .iter()
                        .map(|(t, si)| match boxes[*si] {
                            Some((bw, _)) => bw,
                            None => {
                                let l = &looks[*si];
                                self.m.width(t, l.size, &l.font, l.attrs)
                            }
                        })
                        .collect();
                    let total: f32 = widths.iter().sum();
                    let mut lead = gap.take().unwrap_or(0.0);
                    if cx > x && cx + lead + total > right {
                        line_y += self.flush(&mut run, line_y, lh);
                        cx = x;
                        lead = 0.0;
                    }
                    for ((text, si), mut ww) in segs.into_iter().zip(widths) {
                        let (look, span) = (&looks[si], &spans[si]);
                        if let Some((bw, bh)) = boxes[si] {
                            // The text run before it ends here.
                            if let Some(r) = run.take() {
                                self.pending.push(r);
                            }
                            let target = match &span.link {
                                Some(SpanLink::Embed { target, .. }) => target.clone(),
                                _ => unreachable!("only embeds get boxes"),
                            };
                            self.pending_images.push((target, cx + lead, bw, bh));
                            cx += lead + bw;
                            lead = 0.0;
                            continue;
                        }
                        // A word wider than the whole line (a URL) breaks
                        // by characters rather than overflowing.
                        let mut rest = text;
                        while cx + ww > right && rest.chars().count() > 1 {
                            let cut = self.fit(rest, right - cx, look);
                            let (head, tail) = rest.split_at(cut);
                            let hw = self.m.width(head, look.size, &look.font, look.attrs);
                            self.place(&mut run, head, cx + lead, hw, lead, look, span);
                            line_y += self.flush(&mut run, line_y, lh);
                            cx = x;
                            lead = 0.0;
                            rest = tail;
                            ww = self.m.width(rest, look.size, &look.font, look.attrs);
                        }
                        self.place(&mut run, rest, cx + lead, ww, lead, look, span);
                        cx += lead + ww;
                        lead = 0.0;
                    }
                }
            }
        }
        let last = self.flush(&mut run, line_y, lh);
        if !any && line_y == y {
            return y + lh;
        }
        line_y + last
    }

    /// The box an inline image embed takes in a column `w` wide, when the
    /// host has its image (sized by its `|300` alias, as a block embed).
    pub(super) fn inline_image(&self, span: &Span, w: f32) -> Option<(f32, f32)> {
        let Some(SpanLink::Embed { target, .. }) = &span.link else { return None };
        let img = (self.image)(target)?;
        let want = cce_vault::markdown::embed_size(&span.text);
        Some(img.fit(want.map(|s| s.width), want.and_then(|s| s.height), w))
    }

    /// The longest char prefix of `word` fitting `w` (at least one char).
    pub(super) fn fit(&mut self, word: &str, w: f32, look: &Look) -> usize {
        let mut best = word.chars().next().map(char::len_utf8).unwrap_or(word.len());
        for (i, c) in word.char_indices().skip(1) {
            let end = i + c.len_utf8();
            if self.m.width(&word[..end], look.size, &look.font, look.attrs) > w {
                break;
            }
            best = end;
        }
        best
    }

    pub(super) fn space_width(&mut self, look: &Look) -> f32 {
        let two = self.m.width("x x", look.size, &look.font, look.attrs);
        let one = self.m.width("xx", look.size, &look.font, look.attrs);
        (two - one).max(look.size * 0.2)
    }

    pub(super) fn look(&self, span: &Span, base: RunStyle) -> Look {
        let s = span.style;
        let code = s.code || s.math;
        let size = if code { (base.size * 0.92).round() } else { base.size };
        let color = match &span.link {
            Some(SpanLink::Tag(_)) => crate::color::text_tag_color(),
            Some(l @ (SpanLink::Note { .. } | SpanLink::Embed { .. })) => {
                if (self.resolved)(l) {
                    crate::color::text_link_color()
                } else {
                    crate::color::text_link_unresolved_color()
                }
            }
            Some(SpanLink::Url(_)) => crate::color::text_link_color(),
            None => base.color.unwrap_or(FG),
        };
        Look {
            size,
            font: if code { self.theme.mono_font.clone() } else { self.theme.body_font.clone() },
            attrs: TextAttrs { italic: s.italic, weight: (s.bold || base.bold).then_some(700), ..Default::default() },
            color,
            bg: if code {
                Some(crate::color::text_code_background_color())
            } else if s.highlight {
                Some(crate::color::text_highlight_background_color())
            } else if matches!(span.link, Some(SpanLink::Tag(_))) {
                Some(crate::color::text_tag_background_color())
            } else {
                None
            },
            strike: s.strike,
        }
    }

    /// Add a word to the current run, or start a new one if the look or
    /// link changed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn place(&mut self, run: &mut Option<Run>, word: &str, x: f32, w: f32, lead: f32, look: &Look, span: &Span) {
        if let Some(r) = run {
            if r.look == *look && r.link == span.link && (r.x + r.w + lead - x).abs() < 0.5 {
                if lead > 0.0 {
                    r.text.push(' ');
                }
                r.text.push_str(word);
                r.w += lead + w;
                return;
            }
        }
        // The line is the same until flushed; only a look change ends it.
        if let Some(r) = run.take() {
            self.pending.push(r);
        }
        *run = Some(Run { text: word.to_string(), x, w, look: look.clone(), link: span.link.clone() });
    }

    /// Emit the current line at `line_y`; returns its height — `lh`, or
    /// taller when an inline image is. Text sits on the line's bottom `lh`
    /// band and images stand on the same bottom, as inline images do.
    pub(super) fn flush(&mut self, run: &mut Option<Run>, line_y: f32, lh: f32) -> f32 {
        if let Some(r) = run.take() {
            self.pending.push(r);
        }
        let images = std::mem::take(&mut self.pending_images);
        let height = images.iter().fold(lh, |h, (_, _, _, ih)| h.max(*ih + INLINE_IMAGE_PAD));
        for (target, ix, iw, ih) in images {
            let rect = Rect { x: ix, y: line_y + height - ih - INLINE_IMAGE_PAD / 2.0, width: iw, height: ih };
            self.out.draws.push(Draw::Image { target, rect });
        }
        // Text runs below sit in the bottom band.
        let line_y = line_y + height - lh;
        for r in std::mem::take(&mut self.pending) {
            let ty = line_y + (lh - r.look.size) / 2.0;
            let box_ = Rect { x: r.x - 2.0, y: line_y + 2.0, width: r.w + 4.0, height: lh - 4.0 };
            if let Some(bg) = r.look.bg {
                self.out.draws.push(Draw::Round { rect: box_, radius: 3.0, color: bg });
            }
            self.out.draws.push(Draw::Text {
                text: r.text,
                x: r.x,
                y: ty,
                size: r.look.size,
                color: r.look.color,
                font: r.look.font.clone(),
                attrs: r.look.attrs,
            });
            if r.look.strike {
                let sy = ty + r.look.size * 0.55;
                self.out.draws.push(Draw::Line { x1: r.x, y1: sy, x2: r.x + r.w, y2: sy, width: 1.0, color: r.look.color });
            }
            if let Some(link) = r.link {
                self.out.hits.push((Rect { x: r.x, y: line_y, width: r.w, height: lh }, Hit::Link(link)));
            }
        }
        height
    }
}

pub(super) enum Piece<'a> {
    Word(&'a str),
    Space,
    Break,
}

/// Split text into words, whitespace runs and hard line breaks.
pub(super) fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push(Piece::Word(&text[s..i]));
            }
            if c == '\n' {
                out.push(Piece::Break);
            } else if !matches!(out.last(), Some(Piece::Space)) {
                out.push(Piece::Space);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push(Piece::Word(&text[s..]));
    }
    out
}

/// What wraps as a unit: a word may run across spans (`**bold**,` is one
/// word in two styles), and never breaks between them.
pub(super) enum Tok<'a> {
    /// Segments of one word, each with the index of its span.
    Word(Vec<(&'a str, usize)>),
    /// A space, in the style of the span it sits in.
    Space(usize),
    Break,
}

pub(super) fn tokens(spans: &[Span]) -> Vec<Tok<'_>> {
    let mut out: Vec<Tok> = Vec::new();
    for (si, span) in spans.iter().enumerate() {
        for (pi, piece) in pieces(&span.text).into_iter().enumerate() {
            match piece {
                Piece::Word(w) => match out.last_mut() {
                    // Glued: the previous span ended mid-word.
                    Some(Tok::Word(segs)) if pi == 0 => segs.push((w, si)),
                    _ => out.push(Tok::Word(vec![(w, si)])),
                },
                Piece::Space => out.push(Tok::Space(si)),
                Piece::Break => out.push(Tok::Break),
            }
        }
    }
    out
}
