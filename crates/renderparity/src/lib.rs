//! Compare borrowed sRGB RGBA8 frames without codecs or runtime dependencies.
//!
//! ```
//! use renderparity::{FrameView, AlphaMode, VisualPolicy, compare_rgba8};
//! let pixels = [255, 0, 0, 255];
//! let frame = FrameView::new(&pixels, 1, 1, 4, AlphaMode::Straight)?;
//! assert_eq!(compare_rgba8(frame, frame, VisualPolicy::default())?.mismatched_pixels, 0);
//! # Ok::<(), renderparity::VisualError>(())
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Interpretation of the alpha channel; no implicit conversions are performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlphaMode {
    /// RGB channels are independent of alpha.
    Straight,
    /// RGB channels have already been multiplied by alpha.
    Premultiplied,
}

/// Invalid frame, incompatible metadata, or invalid region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualError {
    /// Zero dimensions, arithmetic overflow, insufficient stride or short buffer.
    InvalidBuffer,
    /// Dimensions differ; resizing is never implicit.
    DimensionMismatch,
    /// Alpha interpretations differ.
    AlphaMismatch,
    /// Empty region or coordinates outside the frame.
    InvalidRegion,
}
impl std::fmt::Display for VisualError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidBuffer => "invalid RGBA8 dimensions, stride or buffer length",
            Self::DimensionMismatch => "frame dimensions differ",
            Self::AlphaMismatch => "frame alpha modes differ",
            Self::InvalidRegion => "region must be nonempty and inside the frame",
        })
    }
}
impl std::error::Error for VisualError {}

/// Validated borrowed sRGB RGBA8 pixels. Row padding is ignored during comparison.
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    data: &'a [u8],
    width: usize,
    height: usize,
    stride: usize,
    alpha: AlphaMode,
}
impl<'a> FrameView<'a> {
    /// Validate a frame. The buffer must contain `height * stride` bytes, including
    /// final-row padding. Extra trailing bytes are allowed and ignored.
    ///
    /// # Errors
    /// Returns InvalidBuffer for zero dimensions, overflow, short stride or data.
    pub fn new(
        data: &'a [u8],
        width: usize,
        height: usize,
        stride: usize,
        alpha: AlphaMode,
    ) -> Result<Self, VisualError> {
        let row = width.checked_mul(4).ok_or(VisualError::InvalidBuffer)?;
        let size = height
            .checked_mul(stride)
            .ok_or(VisualError::InvalidBuffer)?;
        if width == 0 || height == 0 || stride < row || data.len() < size {
            return Err(VisualError::InvalidBuffer);
        }
        Ok(Self {
            data,
            width,
            height,
            stride,
            alpha,
        })
    }
    /// Width in pixels.
    pub const fn width(self) -> usize {
        self.width
    }
    /// Height in pixels.
    pub const fn height(self) -> usize {
        self.height
    }
}

/// Half-open rectangle: `[x, right) × [y, bottom)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelBounds {
    /// Included left coordinate.
    pub x: usize,
    /// Included top coordinate.
    pub y: usize,
    /// Excluded right coordinate.
    pub right: usize,
    /// Excluded bottom coordinate.
    pub bottom: usize,
}

/// Explicit channel tolerance and optional region. Default is exact, full-frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct VisualPolicy {
    /// Maximum allowed absolute difference for each RGBA channel, inclusive.
    pub tolerance: u8,
    /// Symmetric comparison region; None selects the entire frame.
    pub region: Option<PixelBounds>,
}

/// Measurements over the selected region, including alpha and transparent pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
#[must_use]
pub struct VisualComparison {
    /// Number of pixels inspected.
    pub compared_pixels: usize,
    /// Pixels with at least one channel difference above tolerance.
    pub mismatched_pixels: usize,
    /// Mismatched / compared pixels, in [0, 1].
    pub mismatch_fraction: f64,
    /// Largest raw absolute channel error, irrespective of tolerance.
    pub max_channel_error: u8,
    /// Mean raw absolute channel error across all four channels, in [0, 255].
    pub mean_channel_error: f64,
    /// Bounding rectangle of pixels exceeding tolerance, or None for equality.
    pub bounds: Option<PixelBounds>,
}

/// Compare without allocation, decoding, normalization, or color conversion.
/// Metrics are raw sRGB byte differences, not perceptual distances.
///
/// # Errors
/// Incompatible dimensions/alpha modes and empty/out-of-frame regions are rejected.
pub fn compare_rgba8(
    a: FrameView<'_>,
    b: FrameView<'_>,
    policy: VisualPolicy,
) -> Result<VisualComparison, VisualError> {
    if a.width != b.width || a.height != b.height {
        return Err(VisualError::DimensionMismatch);
    }
    if a.alpha != b.alpha {
        return Err(VisualError::AlphaMismatch);
    }
    let region = policy.region.unwrap_or(PixelBounds {
        x: 0,
        y: 0,
        right: a.width,
        bottom: a.height,
    });
    if region.x >= region.right
        || region.y >= region.bottom
        || region.right > a.width
        || region.bottom > a.height
    {
        return Err(VisualError::InvalidRegion);
    }
    let count = (region.right - region.x) * (region.bottom - region.y);
    let mut mismatched = 0;
    let mut max_error = 0;
    let mut sum = 0.0;
    let mut bounds: Option<PixelBounds> = None;
    for y in region.y..region.bottom {
        for x in region.x..region.right {
            let mut different = false;
            for c in 0..4 {
                let error =
                    a.data[y * a.stride + x * 4 + c].abs_diff(b.data[y * b.stride + x * 4 + c]);
                max_error = max_error.max(error);
                sum += f64::from(error);
                different |= error > policy.tolerance;
            }
            if different {
                mismatched += 1;
                bounds = Some(match bounds {
                    None => PixelBounds {
                        x,
                        y,
                        right: x + 1,
                        bottom: y + 1,
                    },
                    Some(r) => PixelBounds {
                        x: r.x.min(x),
                        y: r.y.min(y),
                        right: r.right.max(x + 1),
                        bottom: r.bottom.max(y + 1),
                    },
                });
            }
        }
    }
    Ok(VisualComparison {
        compared_pixels: count,
        mismatched_pixels: mismatched,
        mismatch_fraction: mismatched as f64 / count as f64,
        max_channel_error: max_error,
        mean_channel_error: sum / (count as f64 * 4.0),
        bounds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(data: &[u8], w: usize, h: usize, stride: usize) -> FrameView<'_> {
        FrameView::new(data, w, h, stride, AlphaMode::Straight).unwrap()
    }
    #[test]
    fn validates_layout_and_metadata() {
        for (w, h, s) in [
            (0, 1, 4),
            (1, 0, 4),
            (2, 1, 4),
            (usize::MAX, 1, 4),
            (1, usize::MAX, 4),
            (1, 2, 4),
        ] {
            assert!(FrameView::new(&[0; 4], w, h, s, AlphaMode::Straight).is_err());
        }
        let a = frame(&[0; 4], 1, 1, 4);
        assert_eq!(
            compare_rgba8(a, frame(&[0; 8], 2, 1, 8), VisualPolicy::default()),
            Err(VisualError::DimensionMismatch)
        );
        let b = FrameView::new(&[0; 4], 1, 1, 4, AlphaMode::Premultiplied).unwrap();
        assert_eq!(
            compare_rgba8(a, b, VisualPolicy::default()),
            Err(VisualError::AlphaMismatch)
        );
    }
    #[test]
    fn padding_tolerance_and_bounds() {
        let a = [0; 24];
        let mut b = a;
        b[8] = 255; // row padding
        b[16] = 5; // pixel (1, 1)
        let result = compare_rgba8(
            frame(&a, 2, 2, 12),
            frame(&b, 2, 2, 12),
            VisualPolicy::default(),
        )
        .unwrap();
        assert_eq!(result.mismatched_pixels, 1);
        assert_eq!(
            result.bounds,
            Some(PixelBounds {
                x: 1,
                y: 1,
                right: 2,
                bottom: 2
            })
        );
        assert_eq!(result.mean_channel_error, 5.0 / 16.0);
        let result = compare_rgba8(
            frame(&a, 2, 2, 12),
            frame(&b, 2, 2, 12),
            VisualPolicy {
                tolerance: 5,
                region: None,
            },
        )
        .unwrap();
        assert_eq!(result.mismatched_pixels, 0);
        assert_eq!(result.max_channel_error, 5);
    }
    #[test]
    fn regions_cannot_hide_all_evidence() {
        let f = frame(&[0; 16], 2, 2, 8);
        for region in [
            PixelBounds {
                x: 0,
                y: 0,
                right: 0,
                bottom: 2,
            },
            PixelBounds {
                x: 0,
                y: 0,
                right: 3,
                bottom: 2,
            },
        ] {
            assert_eq!(
                compare_rgba8(
                    f,
                    f,
                    VisualPolicy {
                        tolerance: 0,
                        region: Some(region)
                    }
                ),
                Err(VisualError::InvalidRegion)
            );
        }
    }
    #[test]
    fn deterministic_generated_inputs_match_simple_reference() {
        let mut seed = 7u32;
        for n in 1..100 {
            let mut a = vec![0u8; n * 4];
            let mut b = a.clone();
            for byte in a.iter_mut().chain(b.iter_mut()) {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *byte = (seed >> 24) as u8;
            }
            let tolerance = n as u8;
            let expected = a
                .chunks_exact(4)
                .zip(b.chunks_exact(4))
                .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > tolerance))
                .count();
            let policy = VisualPolicy {
                tolerance,
                region: None,
            };
            let actual =
                compare_rgba8(frame(&a, n, 1, n * 4), frame(&b, n, 1, n * 4), policy).unwrap();
            assert_eq!(actual.mismatched_pixels, expected);
            assert_eq!(
                actual,
                compare_rgba8(frame(&b, n, 1, n * 4), frame(&a, n, 1, n * 4), policy).unwrap()
            );
        }
    }
}
