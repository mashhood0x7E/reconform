# PNG comparisons

Enable the CLI codec with `--features png`; neither pure library needs a codec.
Run `reconform compare-png left.png right.png --plan visual.json --format json`.

```json
{
  "schema_version": "1.0",
  "tolerance": 0,
  "max_mismatch_fraction": 0.0,
  "region": null,
  "assume_srgb": false
}
```

For captured files, select their byte observation in a normal comparison plan and
use the same fields with `"kind":"png"` instead of `schema_version`. Reports
include mismatch count/fraction, maximum and mean channel error and half-open
mismatch bounds. Required image findings participate in the ordinary verdict
gate and ordered first-divergence selection. Without the codec feature, a PNG
policy returns unsupported and cannot satisfy a required gate.

Tolerance is an inclusive per-channel byte difference. The allowed mismatch
fraction is inclusive and must lie in `[0,1]`. A region is `[x,y,right,bottom]`,
applied symmetrically to both images; null compares the full frame. Different
dimensions are inconclusive. Images are never resized or aligned implicitly.
Alpha and RGB beneath fully transparent pixels are compared.

Add `--diff diff.png` to write a fresh diagnostic image. Red intensity is the
maximum raw RGBA channel difference at each selected pixel, independent of the
pass threshold; black means zero difference or outside the selected region.
The report includes its content digest, media type and byte size. Dimension
incompatibility produces no diff. Existing files are never overwritten.

The decoder expands palettes, low-bit grayscale and transparency to straight
RGBA8. RGB and grayscale inputs receive opaque alpha. Animated and 16-bit inputs
are rejected. An sRGB tag is required unless `assume_srgb` explicitly permits
untagged images. ICC, HDR/video color metadata and non-sRGB gamma/chromaticity
profiles are rejected; this implementation performs no color management.

Standalone files are limited to 32 MiB encoded and 64 MiB decoded RGBA each;
decoder internal allocation is separately limited to 64 MiB. Captured images
also obey the smaller bundle/file-capture limits. Malformed, truncated and
checksum-invalid images fail. Metrics measure raw channel differences, not
perceptual similarity or functional program equivalence.
