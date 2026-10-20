use renderparity::{AlphaMode, FrameView, PixelBounds, VisualPolicy, compare_rgba8};
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = vec![0; 1920 * 1080 * 4];
    let mut b = a.clone();
    for y in 100..228 {
        for x in 200..328 {
            b[(y * 1920 + x) * 4] = 255;
        }
    }
    let left = FrameView::new(&a, 1920, 1080, 1920 * 4, AlphaMode::Straight)?;
    let right = FrameView::new(&b, 1920, 1080, 1920 * 4, AlphaMode::Straight)?;
    let result = compare_rgba8(left, right, VisualPolicy::default())?;
    assert_eq!(result.mismatched_pixels, 128 * 128);
    assert_eq!(
        result.bounds,
        Some(PixelBounds {
            x: 200,
            y: 100,
            right: 328,
            bottom: 228
        })
    );
    let mut samples = Vec::new();
    for _ in 0..50 {
        let start = Instant::now();
        let _ = black_box(compare_rgba8(
            black_box(left),
            black_box(right),
            VisualPolicy::default(),
        )?);
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "1080p sparse region, 50 samples: p50={:.3} ms, p95={:.3} ms",
        samples[24], samples[47]
    );
    Ok(())
}
