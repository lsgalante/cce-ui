//! `ParamController`: how a host reads and writes the pane's parameters.

use super::*;

impl ParamController for ParametersBg {
    fn node_params(&self) -> Vec<(String, String, String)> {
        self.display_params.clone()
    }

    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        let mut layout_changed = self.display_params.len() != params.len();
        if !layout_changed {
            for (p_old, p_new) in self.display_params.iter().zip(params.iter()) {
                if p_old.0 != p_new.0 || p_old.2 != p_new.2 {
                    layout_changed = true;
                    break;
                }
            }
        }

        if layout_changed {
            self.scroll_y = 0.0;
            self.display_params = params.to_vec();
            self.focused_param = None;
            // Re-read at every rebuild, so a reloaded style takes effect the
            // next time the rows are built, and geometry and widgets agree;
            // then decided against the cached rect's width, as a resize
            // decides it (`apply_label_layout`).
            self.inline_pref = crate::layout::param_labels_inline();
            self.inline_labels = self.decide_inline();
            let inline = self.inline_labels;
            self.sliders = self.display_params.iter().map(|p| {
                if p.2.starts_with("slider") {
                    let val = p.1.parse::<f32>().unwrap_or(0.0);
                    let (min, max) = parse_slider_range(&p.2);
                    let t = if max - min != 0.0 {
                        ((val - min) / (max - min)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let s = Slider::new().with_value(t).with_range(min, max).with_readout(true).with_decimals(slider_decimals(&p.2)).with_soft(is_soft_row(&p.2));
                    Some(if inline { s } else { s.with_label(&p.0) })
                } else {
                    None
                }
            }).collect();
            self.float3s = self.display_params.iter().map(|p| {
                if is_vec_row(&p.2) {
                    let (min, max) = parse_slider_range(&p.2);
                    let n = vec_row_n(&p.2);
                    let mut f = Float3::new().with_components(n).with_range(min, max).with_trackball(Self::has_trackball(&p.2));
                    f.set_soft(is_soft_row(&p.2));
                    f.set_values_n(&parse_vec_value(&p.1, min, max, n));
                    f.set_view(self.trackball_view);
                    Some(if inline { f } else { f.with_label(&p.0) })
                } else {
                    None
                }
            }).collect();
            self.spinboxes = self.display_params.iter().map(|p| {
                if p.2.starts_with("spinbox") {
                    let (min, max, step) = parse_spinbox_range(&p.2);
                    let val = p.1.parse::<i32>().unwrap_or(min);
                    let sb = Spinbox::new(val, min, max, step);
                    Some(if inline { sb } else { sb.with_label(&p.0) })
                } else {
                    None
                }
            }).collect();
            self.buttons = self.display_params.iter().map(|p| {
                if p.2 == "button" {
                    // Left-aligned, like the toggles below: the rows form one column, and
                    // centered labels made each row's text start at a different x.
                    Some(Button::new(0.0, 0.0, 0.0, 0.0).with_label(&p.0).with_left_align(true))
                } else {
                    None
                }
            }).collect();
            self.choices = self.display_params.iter().map(|p| {
                if p.2.starts_with("choice:") {
                    let options_str = p.2.strip_prefix("choice:").unwrap_or("");
                    let options: Vec<String> = options_str.split(',').map(|s| s.to_string()).collect();
                    let selected = options.iter().position(|o| o == &p.1).unwrap_or(0);
                    let d = Dropdown::new(options, selected);
                    Some(if inline { d } else { d.with_label(&p.0) })
                } else if p.2.starts_with("textpick:") {
                    // The text row's completion picker: a menu-button Dropdown
                    // (fixed glyph, re-fires on repeat picks) beside the box.
                    let options: Vec<String> = p.2.strip_prefix("textpick:").unwrap_or("")
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                    if options.is_empty() {
                        None
                    } else {
                        // A raised face: the picker reads as a BUTTON sitting
                        // in the box's recess, not a bare glyph beside it.
                        Some(
                            Dropdown::new(options, 0)
                                .with_custom_display_text("")
                                .with_raised(true),
                        )
                    }
                } else {
                    None
                }
            }).collect();
            self.texts = self.display_params.iter().map(|p| {
                if is_text_row(&p.2) {
                    let tb = TextBox::new(p.1.clone());
                    Some(if inline { tb } else { tb.with_label(&p.0) })
                } else {
                    None
                }
            }).collect();
            self.toggles = self.display_params.iter().map(|p| {
                if p.2 == "toggle" || p.2 == "checkbox" {
                    let on = p.1.trim().to_lowercase() == "true";
                    let mut t = Toggle::new().with_label(&p.0).with_left_align(true);
                    t.set_toggled(on);
                    Some(t)
                } else {
                    None
                }
            }).collect();
            self.colors = self.display_params.iter().map(|p| {
                if p.2 == "rgba" {
                    // Alpha-carrying param: the full picker, 8-digit hex.
                    let col = parse_hex_to_rgba(&p.1).unwrap_or([255, 255, 255, 255]);
                    let c = ColorSelector::new_rgba(col);
                    Some(if inline { c } else { c.with_label(&p.0) })
                } else if p.2.starts_with("color") || p.2 == "rgb" {
                    let col = parse_hex_to_rgb(&p.1).unwrap_or([255, 255, 255]);
                    let c = ColorSelector::new(col);
                    Some(if inline { c } else { c.with_label(&p.0) })
                } else {
                    None
                }
            }).collect();
            self.ramps = self.display_params.iter().map(|p| {
                if p.2 == "ramp" {
                    let mut rp = Ramp::new();
                    rp.inner_mut().set_spec(&p.1);
                    Some(rp)
                } else {
                    None
                }
            }).collect();
        } else {
            for (i, p_new) in params.iter().enumerate() {
                if Some(i) != self.focused_param && Some(i) != self.dragging_param {
                    self.display_params[i].1 = p_new.1.clone();
                    if let Some(ref mut s) = self.sliders[i] {
                        let (min, max) = parse_slider_range(&p_new.2);
                        // Idempotence guard: hosts push params straight back
                        // after every sync, and re-seeding from the 2-decimal
                        // string quantizes away the slider's sub-tick state —
                        // mid-scroll that snaps the value BACKWARD between
                        // wheel events/glide ticks (visible as jitter). Only
                        // re-seed when the incoming string says something the
                        // current value doesn't (a genuinely external change).
                        let cur_str = format!("{:.*}", slider_decimals(&p_new.2), min + s.value * (max - min));
                        if cur_str != p_new.1 {
                            let val = p_new.1.parse::<f32>().unwrap_or(0.0);
                            let t = if max - min != 0.0 {
                                ((val - min) / (max - min)).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            s.set_value(t);
                        }
                    } else if let Some(ref mut f) = self.float3s[i] {
                        let (min, max) = parse_slider_range(&p_new.2);
                        // Same round-trip guard as the slider row.
                        let cur_str = f.value_string();
                        if cur_str != p_new.1 {
                            let n = vec_row_n(&p_new.2);
                            if f.components() != n {
                                f.set_components(n);
                            }
                            f.set_values_n(&parse_vec_value(&p_new.1, min, max, n));
                        }
                    } else if let Some(ref mut sb) = self.spinboxes[i] {
                        if !sb.editing {
                            let (min, _max, _step) = parse_spinbox_range(&p_new.2);
                            let val = p_new.1.parse::<i32>().unwrap_or(min);
                            sb.value = val;
                        }
                    } else if let Some(ref mut d) = self.choices[i] {
                        if !d.open {
                            if let Some(options_str) = p_new.2.strip_prefix("choice:") {
                                let options: Vec<String> = options_str.split(',').map(|s| s.to_string()).collect();
                                if d.options != options {
                                    d.options = options.clone();
                                }
                                if let Some(idx) = options.iter().position(|o| o == &p_new.1) {
                                    d.selected = idx;
                                }
                            }
                        }
                    } else if let Some(ref mut tb) = self.texts[i] {
                        if !tb.editing {
                            tb.set_value_string(&p_new.1);
                        }
                    } else if let Some(ref mut t) = self.toggles[i] {
                        let on = p_new.1.trim().to_lowercase() == "true";
                        t.set_toggled(on);
                    } else if let Some(ref mut c) = self.colors[i] {
                        if !c.editing {
                            c.set_value_string(&p_new.1);
                        }
                    } else if let Some(ref mut rp) = self.ramps[i] {
                        if !rp.inner().is_dragging_key {
                            rp.set_value_string(&p_new.1);
                        }
                    }
                }
            }
        }
        self.refresh_scroll_metrics();
    }
}
