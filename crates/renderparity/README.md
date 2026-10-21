# renderparity

Compare borrowed sRGB RGBA8 frames without dependencies or decoding.

```rust
use renderparity::{FrameView, AlphaMode, VisualPolicy, compare_rgba8};
let pixels = [255, 0, 0, 255];
let frame = FrameView::new(&pixels, 1, 1, 4, AlphaMode::Straight)?;
let result = compare_rgba8(frame, frame, VisualPolicy::default())?;
assert_eq!(result.mismatched_pixels, 0);
# Ok::<(), renderparity::VisualError>(())
```

Frame construction checks nonzero dimensions, stride, arithmetic overflow and
buffer length. Both frames must have equal dimensions and alpha interpretation.
No scaling, alpha conversion or color transformation occurs. Row padding is ignored.

Default comparison is exact across all RGBA channels, including transparent pixels.
An explicit inclusive channel tolerance and rectangular region can be selected.
Results contain mismatch counts/fraction, raw mean/max channel error, and a half-open
bounding rectangle. Empty regions are rejected. Metrics describe byte differences,
not perceptual similarity. The comparison loop uses constant working space.

PNG codecs, arbitrary masks, clustering and temporal alignment are not implemented.
Experimental API. Minimum supported Rust: 1.90, tested on Windows and Linux.
Licensed under MIT OR Apache-2.0.
