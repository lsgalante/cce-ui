//! Lengths with units, and the one bridge between them and the screen.
//!
//! The toolkit's working unit is and stays the **logical pixel**: every
//! layout node, style slot and widget measure is an `f32` of logical px, as
//! it always was. This module adds the two things that were missing:
//!
//! - [`Len`] — a length that remembers its unit (`px`, `mm`, `cm`, `in`,
//!   `pt`), parsed from config (`width=(mm)2.0`, or the string `"2mm"`)
//!   and resolved to logical px through a [`Metric`].
//! - [`Metric`] — how many logical px one millimetre covers on the display
//!   this process is on, and where that number came from. Measured from the
//!   output's EDID size when the compositor reports one, configured by the
//!   user when EDID lies, forced by `CCE_FORCE_PPI` for headless shadows,
//!   or *assumed* at the CSS convention of 96 logical px per inch when
//!   nothing better is known. The source is carried, not hidden: an
//!   assumed metric is a guess, and anything fabricating from it should
//!   say so.
//!
//! Why the toolkit is not converted to millimetres internally: UI sizes are
//! perceptual and angular, not physical. A hit target should not become 8 mm
//! on a projector three metres away. Documents and fabrication content are
//! the things that live in real units, and they convert at view time. Two
//! domains, one bridge — this one.
//!
//! The process-wide metric lives here ([`metric`] / [`set_metric`]), fed by
//! the window runner from the Wayland output the surface is on, exactly as
//! `scale::scale_factor` is. Style slots carrying a unit resolve through it
//! at every read, so a metric arriving after config load, or changing when
//! the window moves to another display, is honoured without a reload.

use std::fmt;
use std::sync::{OnceLock, RwLock};

/// Millimetres per inch.
pub const MM_PER_INCH: f32 = 25.4;
/// Points per inch (PostScript/CSS points).
pub const PT_PER_INCH: f32 = 72.0;
/// The CSS reference pixel: what a logical px is taken to measure when the
/// display's real size is unknown. Same convention as the `Xft.dpi 96×scale`
/// the compositor writes for Xwayland, so a bare pixel keeps its meaning
/// under the fallback.
pub const ASSUMED_PPI: f32 = 96.0;

/// A length unit. `Px` is the logical pixel; the rest are real-world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    Px,
    Mm,
    Cm,
    M,
    In,
    Pt,
}

impl Unit {
    /// Every unit, in the order a unit toggle should cycle them.
    pub const ALL: [Unit; 6] = [Unit::Px, Unit::Mm, Unit::Cm, Unit::M, Unit::In, Unit::Pt];

    /// The config suffix / KDL type annotation: `px`, `mm`, `cm`, `in`, `pt`.
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Px => "px",
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
            Unit::In => "in",
            Unit::Pt => "pt",
        }
    }

    /// Parse a suffix or KDL type annotation. `None` for anything else, so a
    /// caller can tell "not a unit" from a unit — `(f64)` is not a length.
    pub fn parse(s: &str) -> Option<Unit> {
        match s.trim().to_ascii_lowercase().as_str() {
            "px" => Some(Unit::Px),
            "mm" => Some(Unit::Mm),
            "cm" => Some(Unit::Cm),
            "m" | "metre" | "meter" | "metres" | "meters" => Some(Unit::M),
            "in" | "inch" | "inches" => Some(Unit::In),
            "pt" => Some(Unit::Pt),
            _ => None,
        }
    }

    /// Whether this unit is a real-world length (everything but `Px`).
    pub fn is_physical(self) -> bool {
        !matches!(self, Unit::Px)
    }

    /// Millimetres per one of this unit. `None` for `Px`, whose size depends
    /// on the metric.
    fn mm_per_unit(self) -> Option<f32> {
        match self {
            Unit::Px => None,
            Unit::Mm => Some(1.0),
            Unit::Cm => Some(10.0),
            Unit::M => Some(1000.0),
            Unit::In => Some(MM_PER_INCH),
            Unit::Pt => Some(MM_PER_INCH / PT_PER_INCH),
        }
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.suffix())
    }
}

/// Where a [`Metric`]'s px-per-mm came from — carried so a consumer can tell
/// a measurement from a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricSource {
    /// Computed from the output's reported physical size (EDID via
    /// `wl_output` geometry) and its logical size.
    Measured,
    /// The user's per-output `size_mm` override, forwarded by the compositor
    /// in place of the EDID value.
    Configured,
    /// `CCE_FORCE_PPI` in the environment.
    Forced,
    /// Nothing known: the CSS 96 px/in convention. A guess.
    Assumed,
}

impl MetricSource {
    pub fn as_str(self) -> &'static str {
        match self {
            MetricSource::Measured => "measured",
            MetricSource::Configured => "configured",
            MetricSource::Forced => "forced",
            MetricSource::Assumed => "assumed",
        }
    }
}

/// The bridge between logical pixels and real lengths for one display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metric {
    /// The output scale (logical → physical px), as `scale::scale_factor`.
    pub scale: f32,
    /// Logical px per millimetre.
    pub px_per_mm: f32,
    pub source: MetricSource,
}

impl Metric {
    /// The fallback metric: 96 logical px per inch, flagged as assumed.
    pub fn assumed(scale: f32) -> Self {
        Metric { scale, px_per_mm: ASSUMED_PPI / MM_PER_INCH, source: MetricSource::Assumed }
    }

    /// A metric from a display's logical size and physical size in mm.
    /// `None` when either is unusable (zero, negative, or an implausible
    /// density outside 25–1000 logical px per inch — an EDID that reports
    /// the 16×9 cm a TV likes to claim is a lie, not a measurement).
    pub fn from_sizes(scale: f32, logical_px: (f32, f32), mm: (f32, f32), source: MetricSource) -> Option<Self> {
        if logical_px.0 <= 0.0 || logical_px.1 <= 0.0 || mm.0 <= 0.0 || mm.1 <= 0.0 {
            return None;
        }
        // Average the two axes: EDID rounds each to the millimetre, and a
        // panel's pixels are square, so the mean is closer than either.
        let px_per_mm = 0.5 * (logical_px.0 / mm.0 + logical_px.1 / mm.1);
        Self::from_px_per_mm(scale, px_per_mm, source)
    }

    /// A metric from a density directly, with the same plausibility gate.
    pub fn from_px_per_mm(scale: f32, px_per_mm: f32, source: MetricSource) -> Option<Self> {
        let ppi = px_per_mm * MM_PER_INCH;
        if !ppi.is_finite() || !(25.0..=1000.0).contains(&ppi) {
            return None;
        }
        Some(Metric { scale, px_per_mm, source })
    }

    /// Logical px per inch.
    pub fn ppi(&self) -> f32 {
        self.px_per_mm * MM_PER_INCH
    }

    /// Physical (buffer) px per millimetre.
    pub fn physical_px_per_mm(&self) -> f32 {
        self.px_per_mm * self.scale
    }

    /// Millimetres per logical px.
    pub fn mm_per_px(&self) -> f32 {
        1.0 / self.px_per_mm
    }

    /// Whether this metric was measured or configured, i.e. safe to
    /// dimension real objects from.
    pub fn is_real(&self) -> bool {
        matches!(self.source, MetricSource::Measured | MetricSource::Configured)
    }

    /// Logical px for `value` of `unit`.
    pub fn to_px(&self, value: f32, unit: Unit) -> f32 {
        match unit.mm_per_unit() {
            None => value,
            Some(mm) => value * mm * self.px_per_mm,
        }
    }

    /// `unit` for a length of `px` logical px.
    pub fn from_px(&self, px: f32, unit: Unit) -> f32 {
        match unit.mm_per_unit() {
            None => px,
            Some(mm) => px / (mm * self.px_per_mm),
        }
    }
}

/// A length that remembers its unit. Resolve it with [`Len::resolve`] (or
/// [`Len::px`] against the process metric) exactly once, at the boundary
/// where a config or document value becomes a layout number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Len {
    pub value: f32,
    pub unit: Unit,
}

impl Len {
    pub const fn new(value: f32, unit: Unit) -> Self {
        Len { value, unit }
    }
    pub const fn px(value: f32) -> Self {
        Len::new(value, Unit::Px)
    }
    pub const fn mm(value: f32) -> Self {
        Len::new(value, Unit::Mm)
    }
    pub const fn cm(value: f32) -> Self {
        Len::new(value, Unit::Cm)
    }
    pub const fn m(value: f32) -> Self {
        Len::new(value, Unit::M)
    }
    pub const fn inches(value: f32) -> Self {
        Len::new(value, Unit::In)
    }
    pub const fn pt(value: f32) -> Self {
        Len::new(value, Unit::Pt)
    }

    /// Parse `"2mm"`, `"0.5 in"`, `"12px"`, `"6pt"`. A bare number is
    /// `None`: the caller decides what an unsuffixed number means (in
    /// config it is a logical px and takes the fast path), and this parser
    /// only ever claims a value that *said* its unit.
    pub fn parse(s: &str) -> Option<Len> {
        let s = s.trim();
        let split = s.find(|c: char| c.is_ascii_alphabetic())?;
        let (num, suffix) = s.split_at(split);
        let value = num.trim().parse::<f32>().ok().filter(|v| v.is_finite())?;
        let unit = Unit::parse(suffix)?;
        Some(Len { value, unit })
    }

    /// Parse a number plus a KDL type annotation (`(mm)2.0`): `None` when
    /// the annotation is not a unit.
    pub fn from_annotated(value: f32, annotation: &str) -> Option<Len> {
        Unit::parse(annotation).map(|unit| Len { value, unit })
    }

    /// Logical px under `metric`.
    pub fn resolve(&self, metric: &Metric) -> f32 {
        metric.to_px(self.value, self.unit)
    }

    /// Logical px under the process metric.
    pub fn to_px(&self) -> f32 {
        self.resolve(&metric())
    }

    /// The same length expressed in `unit` under `metric`.
    pub fn convert(&self, unit: Unit, metric: &Metric) -> Len {
        Len { value: metric.from_px(self.resolve(metric), unit), unit }
    }

    /// The compact form `parse` reads back: `2mm`, `9.3px`. Trailing zeros
    /// trimmed so a config line stays as the user typed it.
    pub fn serialize(&self) -> String {
        format!("{}{}", fmt_num(self.value), self.unit.suffix())
    }
}

impl fmt::Display for Len {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.serialize())
    }
}

/// A number with up to four decimals, trailing zeros dropped.
pub fn fmt_num(v: f32) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".to_string() } else { s.to_string() }
}

static METRIC: RwLock<Metric> = RwLock::new(Metric {
    scale: 1.0,
    px_per_mm: ASSUMED_PPI / MM_PER_INCH,
    source: MetricSource::Assumed,
});
static FORCED_PPI: OnceLock<Option<f32>> = OnceLock::new();

/// `CCE_FORCE_PPI=<logical px per inch>`: pin the metric regardless of what
/// the outputs report — a headless shadow has no EDID and would otherwise
/// run assumed, so a test that measures a millimetre sets this to the live
/// panel's figure (141.8 on the 3840×2400 / 344 mm laptop at scale 2).
pub fn forced_ppi() -> Option<f32> {
    *FORCED_PPI.get_or_init(|| {
        std::env::var("CCE_FORCE_PPI")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|p| p.is_finite() && *p > 0.0)
    })
}

/// The process-wide metric.
pub fn metric() -> Metric {
    *METRIC.read().unwrap()
}

/// Install the process-wide metric. A forced PPI overrides everything but
/// keeps the caller's scale. Called by the window runner as outputs come and
/// go; apps only read.
pub fn set_metric(m: Metric) {
    let m = match forced_ppi() {
        Some(ppi) => Metric { scale: m.scale, px_per_mm: ppi / MM_PER_INCH, source: MetricSource::Forced },
        None => m,
    };
    if let Ok(mut lock) = METRIC.write() {
        if *lock != m {
            log::info!(
                "[units] metric: {:.3} logical px/mm ({:.1} ppi, scale {}) — {}",
                m.px_per_mm,
                m.ppi(),
                m.scale,
                m.source.as_str()
            );
        }
        *lock = m;
    }
}

/// Logical px per millimetre under the process metric.
pub fn px_per_mm() -> f32 {
    metric().px_per_mm
}

/// Logical px for `v` millimetres under the process metric.
pub fn mm(v: f32) -> f32 {
    metric().to_px(v, Unit::Mm)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live laptop panel: 3840×2400 over 344×215 mm at scale 2.
    fn panel() -> Metric {
        Metric::from_sizes(2.0, (1920.0, 1200.0), (344.0, 215.0), MetricSource::Measured).unwrap()
    }

    #[test]
    fn panel_metric_is_about_5_6_px_per_mm() {
        let m = panel();
        assert!((m.px_per_mm - 5.58).abs() < 0.02, "{}", m.px_per_mm);
        assert!((m.ppi() - 141.8).abs() < 0.5);
        assert!((m.physical_px_per_mm() - 11.16).abs() < 0.05);
        assert!(m.is_real());
    }

    #[test]
    fn assumed_is_css_px() {
        let m = Metric::assumed(1.0);
        assert_eq!(Len::inches(1.0).resolve(&m), 96.0);
        assert!((Len::pt(72.0).resolve(&m) - 96.0).abs() < 1e-4);
        assert!(!m.is_real());
    }

    #[test]
    fn implausible_sizes_reject() {
        // An EDID claiming 16×9 mm at 4K: 240 px/mm, nonsense. (The gate is
        // deliberately wide — a 4K panel over 16×9 *cm* is 610 ppi, which a
        // phone-class panel can be — so only the absurd is refused; the
        // `size_mm` override exists for the merely wrong.)
        assert!(Metric::from_sizes(1.0, (3840.0, 2160.0), (16.0, 9.0), MetricSource::Measured).is_none());
        assert!(Metric::from_sizes(1.0, (1920.0, 1080.0), (0.0, 0.0), MetricSource::Measured).is_none());
    }

    #[test]
    fn parse_and_serialize_roundtrip() {
        for s in ["2mm", "0.5in", "12px", "6pt", "1.25cm", "0.3m"] {
            let l = Len::parse(s).unwrap();
            assert_eq!(l.serialize(), s, "{s}");
        }
        assert_eq!(Len::parse("2 mm"), Some(Len::mm(2.0)));
        assert_eq!(Len::parse("2"), None, "bare numbers are the caller's");
        assert_eq!(Len::parse("2em"), None);
        assert_eq!(Len::parse("mm"), None);
        assert_eq!(Len::from_annotated(2.0, "mm"), Some(Len::mm(2.0)));
        assert_eq!(Len::from_annotated(2.0, "f64"), None);
    }

    #[test]
    fn resolve_and_convert() {
        let m = panel();
        let roll = Len::px(9.3);
        let in_mm = roll.convert(Unit::Mm, &m);
        assert!((in_mm.value - 1.67).abs() < 0.01, "{in_mm}");
        assert!((Len::mm(1.0).resolve(&m) - 5.58).abs() < 0.02);
        assert_eq!(Len::px(4.0).resolve(&m), 4.0);
    }

    #[test]
    fn fmt_num_trims() {
        assert_eq!(fmt_num(2.0), "2");
        assert_eq!(fmt_num(9.3), "9.3");
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(1.23456), "1.2346");
    }
}
