use crate::{Error, Options, digest, emit, model::VERSION, read, read_bytes};
use renderparity::{AlphaMode, FrameView, PixelBounds, VisualPolicy, compare_rgba8};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Cursor;

const MAX_ENCODED: u64 = 32 * 1024 * 1024;
const MAX_DECODED: usize = 64 * 1024 * 1024;

pub fn compare_encoded(
    a: &[u8],
    b: &[u8],
    tolerance: u8,
    fraction: f64,
    region: Option<[usize; 4]>,
    assume_srgb: bool,
) -> (
    reconform_core::Verdict,
    &'static str,
    Option<serde_json::Value>,
) {
    use reconform_core::Verdict;
    let (Ok(a), Ok(b)) = (decode(a, assume_srgb), decode(b, assume_srgb)) else {
        return (Verdict::Error, "PNG_DECODE_ERROR", None);
    };
    let (Ok(a), Ok(b)) = (a.view(), b.view()) else {
        return (Verdict::Error, "PNG_DECODE_ERROR", None);
    };
    match compare_rgba8(
        a,
        b,
        VisualPolicy {
            tolerance,
            region: region.map(|[x, y, right, bottom]| PixelBounds {
                x,
                y,
                right,
                bottom,
            }),
        },
    ) {
        Ok(m) => (
            if m.mismatch_fraction <= fraction {
                Verdict::Pass
            } else {
                Verdict::Fail
            },
            "PIXEL_COMPARISON",
            Some(
                json!({"compared_pixels":m.compared_pixels,"mismatched_pixels":m.mismatched_pixels,"mismatch_fraction":m.mismatch_fraction,"max_channel_error":m.max_channel_error,"mean_channel_error":m.mean_channel_error,"bounds":m.bounds.map(|b| [b.x,b.y,b.right,b.bottom])}),
            ),
        ),
        Err(renderparity::VisualError::DimensionMismatch) => {
            (Verdict::Inconclusive, "DIMENSION_MISMATCH", None)
        }
        Err(_) => (Verdict::Error, "PNG_REGION_INVALID", None),
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema_version: String,
    tolerance: u8,
    max_mismatch_fraction: f64,
    region: Option<[usize; 4]>,
    // Untagged images require an explicit declaration by the caller.
    assume_srgb: bool,
}

struct Frame {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
}
impl Frame {
    fn view(&self) -> Result<FrameView<'_>, Error> {
        FrameView::new(
            &self.pixels,
            self.width,
            self.height,
            self.width * 4,
            AlphaMode::Straight,
        )
        .map_err(|e| Error::config(e.to_string()))
    }
}

fn decode(bytes: &[u8], assume_srgb: bool) -> Result<Frame, Error> {
    let mut decoder =
        png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: MAX_DECODED });
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|_| Error::config("invalid PNG header"))?;
    let info = reader.info();
    if info.animation_control.is_some() || info.bit_depth == png::BitDepth::Sixteen {
        return Err(Error::config(
            "animated and 16-bit PNG inputs are unsupported",
        ));
    }
    if info.icc_profile.is_some()
        || info.coding_independent_code_points.is_some()
        || info.mastering_display_color_volume.is_some()
        || (info.srgb.is_none()
            && (info.source_gamma.is_some()
                || info.source_chromaticities.is_some()
                || !assume_srgb))
    {
        return Err(Error::config(
            "PNG must declare sRGB or be untagged with assume_srgb enabled; color conversion is unsupported",
        ));
    }
    let width = info.width as usize;
    let height = info.height as usize;
    let size = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= MAX_DECODED)
        .ok_or_else(|| Error::config("PNG exceeds decoded pixel budget"))?;
    let output = reader
        .output_buffer_size()
        .filter(|n| *n <= MAX_DECODED)
        .ok_or_else(|| Error::config("PNG exceeds decoder output budget"))?;
    let mut buffer = vec![0; output];
    let decoded = reader
        .next_frame(&mut buffer)
        .map_err(|_| Error::config("invalid PNG pixels or checksum"))?;
    reader
        .finish()
        .map_err(|_| Error::config("invalid PNG end or checksum"))?;
    let mut pixels = Vec::with_capacity(size);
    match decoded.color_type {
        png::ColorType::Rgba => pixels.extend_from_slice(&buffer[..decoded.buffer_size()]),
        png::ColorType::Rgb => {
            for p in buffer[..decoded.buffer_size()].chunks_exact(3) {
                pixels.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for p in &buffer[..decoded.buffer_size()] {
                pixels.extend_from_slice(&[*p, *p, *p, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for p in buffer[..decoded.buffer_size()].chunks_exact(2) {
                pixels.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }
        png::ColorType::Indexed => return Err(Error::config("unexpanded PNG palette")),
    }
    Ok(Frame {
        pixels,
        width,
        height,
    })
}

pub fn execute(opts: &Options) -> Result<u8, Error> {
    if let Some(path) = &opts.diff
        && (std::path::Path::new(path).exists() || opts.output.as_ref() == Some(path))
    {
        return Err(Error::config(
            "diff must be a fresh file distinct from the report",
        ));
    }
    if opts.positional.len() != 2 || opts.kind.is_some() {
        return Err(Error::config(
            "compare-png requires LEFT RIGHT --plan POLICY",
        ));
    }
    let path = opts
        .plan
        .as_deref()
        .ok_or_else(|| Error::config("compare-png requires --plan POLICY"))?;
    let (policy, policy_id) = read::<Policy>(path)?;
    if policy.schema_version != VERSION
        || !policy.max_mismatch_fraction.is_finite()
        || !(0.0..=1.0).contains(&policy.max_mismatch_fraction)
    {
        return Err(Error::config(
            "invalid PNG policy version or mismatch fraction",
        ));
    }
    let (left_bytes, left_id) = read_bytes(&opts.positional[0], MAX_ENCODED)?;
    let (right_bytes, right_id) = read_bytes(&opts.positional[1], MAX_ENCODED)?;
    let left = decode(&left_bytes, policy.assume_srgb)?;
    let right = decode(&right_bytes, policy.assume_srgb)?;
    let visual = VisualPolicy {
        tolerance: policy.tolerance,
        region: policy.region.map(|[x, y, right, bottom]| PixelBounds {
            x,
            y,
            right,
            bottom,
        }),
    };
    let result = compare_rgba8(left.view()?, right.view()?, visual);
    let (verdict, code, metrics, exit) = match result {
        Ok(m) => {
            let pass = m.mismatch_fraction <= policy.max_mismatch_fraction;
            (
                if pass { "pass" } else { "fail" },
                "PIXEL_COMPARISON",
                json!({"compared_pixels":m.compared_pixels,"mismatched_pixels":m.mismatched_pixels,"mismatch_fraction":m.mismatch_fraction,"max_channel_error":m.max_channel_error,"mean_channel_error":m.mean_channel_error,"bounds":m.bounds.map(|b| [b.x,b.y,b.right,b.bottom])}),
                if pass { 0 } else { 1 },
            )
        }
        Err(renderparity::VisualError::DimensionMismatch) => (
            "inconclusive",
            "DIMENSION_MISMATCH",
            serde_json::Value::Null,
            4,
        ),
        Err(e) => return Err(Error::config(e.to_string())),
    };
    let report = json!({"schema_version":VERSION,"tool_version":env!("CARGO_PKG_VERSION"),"method":"png-parity/1","verdict":verdict,"code":code,"policy_digest":digest(&serde_json::to_vec(&policy).map_err(|_| Error::config("policy serialization failed"))?),"policy":policy,"inputs":[left_id,right_id,policy_id],"measurements":metrics,"dimensions":[[left.width,left.height],[right.width,right.height]],"limitations":["Raw sRGB channel differences are not perceptual equivalence.","Alpha and RGB beneath transparent pixels are compared."],"reproduce":["reconform","compare-png",left_id.path,right_id.path,"--plan",policy_id.path,"--format","json"]});
    let mut report = report;
    if let Some(path) = &opts.diff
        && exit != 4
    {
        let mut pixels = vec![0; left.pixels.len()];
        let [x, y, r, b] = policy.region.unwrap_or([0, 0, left.width, left.height]);
        for (index, ((a, bp), out)) in left
            .pixels
            .chunks_exact(4)
            .zip(right.pixels.chunks_exact(4))
            .zip(pixels.chunks_exact_mut(4))
            .enumerate()
        {
            let px = index % left.width;
            let py = index / left.width;
            out[3] = 255;
            if px >= x && px < r && py >= y && py < b {
                out[0] = a
                    .iter()
                    .zip(bp)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
            }
        }
        let mut encoded = Vec::new();
        let mut encoder = png::Encoder::new(&mut encoded, left.width as u32, left.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = encoder
            .write_header()
            .map_err(|_| Error::config("cannot encode diff header"))?;
        writer
            .write_image_data(&pixels)
            .map_err(|_| Error::config("cannot encode diff pixels"))?;
        writer
            .finish()
            .map_err(|_| Error::config("cannot finish diff image"))?;
        crate::write_new(std::path::Path::new(path), &encoded)?;
        report["diff"] = json!({"path":path,"digest":digest(&encoded),"media_type":"image/png","size_bytes":encoded.len()});
    }
    emit(
        &report,
        &format!("PNG comparison: {verdict} ({code})\n"),
        opts,
    )?;
    Ok(exit)
}
