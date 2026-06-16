---
title: 'Format Guides'
description: 'Choosing quality, speed and chroma for WebP, AVIF, PNG and JPEG.'
---

# Format Guides

Real numbers from a single 1.2 MB source image, so the trade-offs are concrete:

| Operation                                                  | Result    |
| ---------------------------------------------------------- | --------- |
| `losslessCompressPng` (PNG → PNG)                          | 1.2M → 876K |
| `pngQuantize({ maxQuality: 75 })` (PNG → PNG)              | 1.2M → 228K |
| `webpLossless()` (PNG → WebP)                              | 1.2M → 676K |
| `webp(75)` (PNG → WebP)                                    | 1.2M → 84K  |
| `avif({ quality: 100 })` (PNG → AVIF)                      | 1.2M → 584K |
| `avif({ quality: 75, chromaSubsampling: Yuv420 })`         | 1.2M → 112K |

And for JPEG, on a 192 KB source:

| Operation                | Result    |
| ------------------------ | --------- |
| `compressJpeg()`         | 192K → 184K |
| `compressJpeg({ quality: 75 })` | 192K → 104K |

## WebP

Two modes:

- **Lossy** — `webp(qualityFactor)`, 0–100. The single most effective "make my photos small" lever. `75` is a great default; `84K` from a `1.2M` PNG above.
- **Lossless** — `webpLossless()`. Use for graphics/screenshots where you can't tolerate artifacts but still want better-than-PNG sizes.

```ts
await new Transformer(png).webp(75)        // lossy
await new Transformer(png).webpLossless()  // lossless
```

## AVIF

The smallest files at a given quality, at the cost of CPU. Tune with [`AvifConfig`](/docs/api#avifconfig):

- **`quality`** (0–100, 100 = lossless). `75` is a strong web default.
- **`chromaSubsampling`** — defaults to `Yuv444` (full chroma). Switch to `Yuv420` for photos to roughly halve the file again (`584K → 112K` above) with little perceptible loss.
- **`speed`** (1–10, default 4) — lower is slower but smaller/better. Raise it for faster encodes when iterating.
- **`threads`** — `0` matches CPU cores.

```ts
import { Transformer, ChromaSubsampling } from '@napi-rs/image'

await new Transformer(png).avif({
  quality: 75,
  chromaSubsampling: ChromaSubsampling.Yuv420,
  speed: 4,
})
```

> In the browser ([Playground](/playground)) AVIF encoding is single-threaded; on Node it uses the thread pool.

## PNG

Two distinct tools:

- **`losslessCompressPng`** (oxipng) — pixel-perfect, no quality loss. Strips redundancy, tries filters, recodes the IDAT. Best for graphics you must keep exact. Set `strip: true` to drop non-critical chunks (metadata) for extra savings.
- **`pngQuantize`** (libimagequant) — *lossy* palette quantization. Dramatically smaller (`1.2M → 228K`) for images that tolerate a 256-color palette. Tune `minQuality`/`maxQuality`; lower `maxQuality` = smaller.

```ts
await losslessCompressPng(png, { strip: true })
await pngQuantize(png, { maxQuality: 75 })
```

For *encoding* a decoded image to PNG (not in-place compression), `Transformer.png()` takes [`PngEncodeOptions`](/docs/api#pngencodeoptions): `compressionType` (`Default`/`Fast`/`Best`) and `filterType`.

## JPEG

- **`Transformer.jpeg(quality)`** — encode a decoded image to JPEG. Default quality **90**.
- **`compressJpeg(bytes, opts)`** — MozJPEG re-compress existing JPEG bytes. Default quality **100** (near-lossless); set `quality: 75` for real savings. `optimizeScans` (default `true`) makes progressive files smaller.

```ts
await new Transformer(input).jpeg(82)          // encode at q82
await compressJpeg(jpegBytes, { quality: 75 }) // recompress in place
```

## Picking a format

```
Photographs         → AVIF (smallest) or WebP lossy (faster, universal support)
Graphics / UI / PNG → losslessCompressPng (exact) or pngQuantize (smaller, lossy)
Existing JPEGs       → compressJpeg({ quality }) to shrink without re-decoding pipelines
Need it everywhere   → WebP lossy — broadest browser + tooling support of the modern formats
```

See the [API Reference](/docs/api) for every option, and the [Playground](/playground) to compare formats on your own image.
