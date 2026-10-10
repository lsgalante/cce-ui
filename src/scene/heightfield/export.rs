//! Asking for a height map — an app's `request`, or `CCE_HEIGHTMAP` / `CCE_HEIGHTMAP_MM` in the
//! environment, honoured on the third frame — and writing one out: a 16-bit greyscale PNG and
//! its `.json` sidecar (pitch, range in mm, datum, the metric's source).

use super::*;

/// One export request: where to write, and at what pitch (`None` = one
/// sample per physical px).
#[derive(Debug, Clone)]
pub struct Request {
    pub path: PathBuf,
    pub mm_per_sample: Option<f32>,
}

static REQUEST: Mutex<Option<Request>> = Mutex::new(None);

static FRAMES: AtomicU32 = AtomicU32::new(0);

/// The frame the environment request fires on: layout has settled and the
/// output metric has arrived by then.
const ENV_FRAME: u32 = 3;

/// Ask the runner to export the next frame's height field.
pub fn request(path: impl Into<PathBuf>, mm_per_sample: Option<f32>) {
    if let Ok(mut r) = REQUEST.lock() {
        *r = Some(Request { path: path.into(), mm_per_sample });
    }
}

/// The runner's per-frame poll: an explicit [`request`], or the environment's
/// `CCE_HEIGHTMAP` once, on frame [`ENV_FRAME`].
pub(crate) fn take_request() -> Option<Request> {
    let n = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    if n == ENV_FRAME {
        if let Ok(path) = std::env::var("CCE_HEIGHTMAP") {
            if !path.is_empty() {
                let mm = std::env::var("CCE_HEIGHTMAP_MM").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| *v > 0.0);
                request(path, mm);
            }
        }
    }
    REQUEST.lock().ok().and_then(|mut r| r.take())
}

/// Write the field as a 16-bit greyscale PNG (0 = lowest, 65535 = highest)
/// plus a `<path>.json` sidecar carrying what the PNG cannot: the sample
/// pitch and the height range in mm, the datum, and whether the metric
/// behind those millimetres was measured or only assumed.
pub fn export_png(hf: &HeightField, path: &Path, mm_per_sample: Option<f32>) -> std::io::Result<()> {
    let (data, w, h, pitch_mm) = hf.resampled(mm_per_sample);
    let (lo, hi) = data.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let span = (hi - lo).max(1e-6);
    let mut bytes = Vec::with_capacity(w * h * 2);
    for v in &data {
        let q = (((v - lo) / span) * 65535.0).round().clamp(0.0, 65535.0) as u16;
        bytes.extend_from_slice(&q.to_be_bytes());
    }
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Sixteen);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(&bytes).map_err(std::io::Error::other)?;
    writer.finish().map_err(std::io::Error::other)?;
    let side = serde_json::json!({
        "width": w,
        "height": h,
        "mm_per_sample": pitch_mm,
        "px_per_mm_physical": hf.px_per_mm,
        "min_mm": lo / hf.px_per_mm,
        "max_mm": hi / hf.px_per_mm,
        "datum": "0 = the window's base surface; values are heights above it, +z out of the screen",
        "png": "16-bit greyscale, 0 = min_mm, 65535 = max_mm, linear",
        "metric_source": hf.source.as_str(),
        "metric_is_real": matches!(hf.source, MetricSource::Measured | MetricSource::Configured),
    });
    let mut side_path = path.as_os_str().to_owned();
    side_path.push(".json");
    std::fs::write(side_path, serde_json::to_string_pretty(&side).unwrap_or_default())?;
    Ok(())
}
