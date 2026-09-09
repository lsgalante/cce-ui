//! The `(relief)` config value type: one relief material folded into a
//! single string, so any config key can carry its own material and
//! `cce-relief --key <dotted.key>` can edit it in place.
//!
//! Format: whitespace-separated `name=value` pairs, e.g.
//!
//! ```text
//! w=6.0 d=0.350 k=0.500,0.500,0.500 p=0.000:0.000,0.032:0.001,...,1.000:1.000
//! ```
//!
//! - `w` — wall/roll width in logical px (required)
//! - `h` — the wall's geometric drop, a length: `h=0.5mm`, `h=4px`, or a
//!   bare number of logical px (optional; absent = follow the width at the
//!   analytic ratio, `relief_shade::RECESS_DEPTH` × width)
//! - `d` — light strength across the wall (`bevel_depth`; optional). Kept
//!   as `d` on the wire for every reader already out there; `l` is read as
//!   an alias. It is NOT a length — `h` is.
//! - `k` — the editor's Shoulder/Base/Bias knob triple, a ride-along seed
//!   so `cce-relief` reopens where it was left (optional)
//! - `p` — the wall's height curve as the same ramp spec
//!   `style.surface.relief.profile` carries (optional; absent or the
//!   identity sentinel = the analytic profile)
//!
//! No value contains whitespace (ramp specs are `;`/`:`/`,`-delimited), so
//! parsing is a plain split. Unknown pairs are skipped, not errors —
//! forward compatibility for future fields.

/// One parsed `(relief)` value. See the module doc for the string format.
#[derive(Debug, Clone, PartialEq)]
pub struct ReliefSpec {
    /// Wall/roll width in logical px.
    pub width: f32,
    /// The wall's drop, a length; `None` = follow the width at the analytic
    /// ratio.
    pub height: Option<crate::units::Len>,
    /// Light strength (`bevel_depth`); `None` = keep the process's material.
    pub light: Option<f32>,
    /// Shoulder/Base/Bias editor knobs behind `profile` — seed only, the
    /// renderer never reads them.
    pub knobs: Option<(f32, f32, f32)>,
    /// Wall profile ramp spec; `None` = the analytic profile. May carry the
    /// identity sentinel verbatim — installers filter it like the config
    /// loader does.
    pub profile: Option<String>,
}

impl ReliefSpec {
    /// Parse a `(relief)` value. `None` when `w=` is absent or malformed —
    /// a spec without a width says nothing drawable.
    pub fn parse(s: &str) -> Option<Self> {
        let mut width = None;
        let mut height = None;
        let mut light = None;
        let mut knobs = None;
        let mut profile = None;
        for tok in s.split_whitespace() {
            let Some((k, v)) = tok.split_once('=') else { continue };
            match k {
                "w" => width = v.parse::<f32>().ok().filter(|w| w.is_finite() && *w >= 0.0),
                "d" | "l" => light = v.parse::<f32>().ok().filter(|d| d.is_finite()),
                "h" => {
                    height = crate::units::Len::parse(v)
                        .or_else(|| v.parse::<f32>().ok().map(crate::units::Len::px))
                        .filter(|l| l.value.is_finite() && l.value > 0.0);
                }
                "k" => {
                    let mut it = v.splitn(3, ',').map(|p| p.parse::<f32>().ok());
                    if let (Some(Some(a)), Some(Some(b)), Some(Some(c))) =
                        (it.next(), it.next(), it.next())
                    {
                        knobs = Some((a, b, c));
                    }
                }
                "p" => profile = Some(v.to_string()),
                _ => {}
            }
        }
        Some(Self { width: width?, height, light, knobs, profile })
    }

    /// The string `parse` reads back — what `cce-relief --key` saves.
    pub fn serialize(&self) -> String {
        let mut out = format!("w={:.2}", self.width);
        if let Some(h) = self.height {
            out.push_str(&format!(" h={}", h.serialize()));
        }
        if let Some(d) = self.light {
            out.push_str(&format!(" d={d:.3}"));
        }
        if let Some((a, b, c)) = self.knobs {
            out.push_str(&format!(" k={a:.3},{b:.3},{c:.3}"));
        }
        if let Some(p) = &self.profile {
            out.push_str(&format!(" p={p}"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_full() {
        let spec = ReliefSpec {
            width: 6.0,
            height: Some(crate::units::Len::mm(0.5)),
            light: Some(0.35),
            knobs: Some((0.8, 0.2, 0.5)),
            profile: Some("0.000:0.000,0.500:0.700,1.000:1.000".to_string()),
        };
        assert_eq!(ReliefSpec::parse(&spec.serialize()), Some(spec));
    }

    #[test]
    fn width_only_and_unknown_pairs_skip() {
        let spec = ReliefSpec::parse("w=4 future=stuff junk").unwrap();
        assert_eq!(spec.width, 4.0);
        assert_eq!(spec.height, None);
        assert_eq!(spec.light, None);
        assert_eq!(spec.knobs, None);
        assert_eq!(spec.profile, None);
    }

    #[test]
    fn height_and_light_aliases() {
        let s = ReliefSpec::parse("w=6 h=3 l=0.2").unwrap();
        assert_eq!(s.height, Some(crate::units::Len::px(3.0)), "bare h is logical px");
        assert_eq!(s.light, Some(0.2), "l is read as d");
        let s = ReliefSpec::parse("w=6 h=0.5mm d=0.1").unwrap();
        assert_eq!(s.height, Some(crate::units::Len::mm(0.5)));
        assert_eq!(s.serialize(), "w=6.00 h=0.5mm d=0.100");
        assert_eq!(ReliefSpec::parse("w=6 h=-1").unwrap().height, None);
    }

    #[test]
    fn missing_or_bad_width_rejects() {
        assert_eq!(ReliefSpec::parse("d=0.3"), None);
        assert_eq!(ReliefSpec::parse("w=-1"), None);
        assert_eq!(ReliefSpec::parse(""), None);
    }
}
