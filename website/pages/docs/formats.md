---
title: 'Format Guides'
description: 'Choose image formats and encoder settings, including HEIC platform limits.'
---

# Format Guides

Use the [support table](/docs#supported-formats) to check input and output formats. Compare output size and visual quality on your own images: a quality value of `75` is specific to an encoder, not a shared quality scale across formats or libraries.

## WebP

- **Lossy:** `webp(qualityFactor)` accepts 0–100 and defaults to 90. Try 75–80 as a starting point, then inspect the output.
- **Lossless:** `webpLossless()` preserves decoded pixels. Useful for graphics and screenshots; compare its size with PNG on your inputs.

```ts
await new Transformer(png).webp(75)
await new Transformer(png).webpLossless()
```

Both WebP decoding and encoding are supported. The API works with still images and does not expose animation controls.

## AVIF

AVIF supports both input and output. Encoding uses libavif with the AOM codec. It can produce compact files, but the quality/size/time trade-off depends on the input and settings.

```ts
import { Transformer, ChromaSubsampling } from '@napi-rs/image'

const avif = await new Transformer(png).avif({
  quality: 75,
  alphaQuality: 90,
  chromaSubsampling: ChromaSubsampling.Yuv420,
  speed: 5,
})
```

- **`quality`:** 0–100, default 80. Quality 100 alone does not guarantee a pixel-identical RGB/RGBA round trip: color conversion, chroma subsampling and alpha quality also matter.
- **`alphaQuality`:** 0–100, default 90; controlled independently of color quality.
- **`chromaSubsampling`:** `Yuv444` by default. `Yuv420` reduces chroma resolution, which can reduce size but may affect colored edges and text. Compare it visually before choosing it.
- **`speed`:** 1–10, with 5 used by the 1.15.0 implementation. Higher values favor encoding speed over compression efficiency. Set it explicitly for reproducible comparisons.
- **`threads`:** defaults to the CPU count; `0` requests automatic selection by the codec. Tune this together with the number of images processed concurrently.

The published decoder produces 8-bit pixels, and RGB/RGBA encoding converts higher-bit-depth inputs to 8-bit. Do not use this path when you need to preserve 10/16-bit color precision. In the browser, use the [async Worker integration](/docs#browser-webassembly); it supports threaded WASM and is not a main-thread script.

## PNG

- **`losslessCompressPng`:** optimize existing PNG bytes with oxipng. `strip: true` removes non-critical chunks, including metadata.
- **`pngQuantize`:** lossy palette quantization, including alpha. Tune `minQuality` / `maxQuality` and inspect the result. Smaller output is not guaranteed for every input.
- **`Transformer.png()`:** encode a transformed image to PNG with [`PngEncodeOptions`](/docs/api#pngencodeoptions).

```ts
import { losslessCompressPng, pngQuantize } from '@napi-rs/image'

await losslessCompressPng(png, { strip: true })
await pngQuantize(png, { maxQuality: 75 })
```

## JPEG

- **`Transformer.jpeg(quality)`:** encode a decoded image to JPEG; default quality 90.
- **`compressJpeg(bytes)`:** lossless coefficient optimization of an existing JPEG via MozJPEG; default quality 100. This preserves the compressed image's coefficients, not its original file bytes or all metadata.
- **`compressJpeg(bytes, { quality: 75 })`:** decode and re-encode a JPEG with the image crate's JPEG encoder. This is a lossy path; `optimizeScans` applies to the default MozJPEG coefficient-optimization path.

```ts
await new Transformer(input).jpeg(82)
await compressJpeg(jpegBytes)
await compressJpeg(jpegBytes, { quality: 75 })
```

## HEIC and HEIF

HEVC-in-HEIF input (`.heic` / `.heif`) and HEIC output are available only on **macOS and Windows**, using OS codecs. The package does not bundle a HEVC codec. Linux, Android, FreeBSD and browser/WASM builds reject HEIC operations.

```ts
import { Transformer } from '@napi-rs/image'

const jpeg = await new Transformer(heicBytes).rotate().jpeg(80)
const heic = await new Transformer(pngBytes).heic({ quality: 80 })
```

| Behavior          | macOS (ImageIO)                                          | Windows (WIC)                                                    |
| ----------------- | -------------------------------------------------------- | ---------------------------------------------------------------- |
| Codec requirement | OS ImageIO HEIC support                                  | HEIF Image Extensions and HEVC Video Extensions available to WIC |
| Decode            | 8-bit → RGBA8; 10-bit → RGBA16                           | Always RGBA8, including 10-bit sources                           |
| Encode bit depth  | 8 or 10; 16-bit input defaults to 10                     | 8 only; `bitDepth: 10` rejects                                   |
| Encode alpha      | OS encoder behavior                                      | Flattened to opaque                                              |
| Quality           | Default 80; 90–100 share a 0.9 ceiling; no lossless mode | Default 80; maps 0–100 to the OS encoder's 0–1 scale             |

Do not assume the Windows extensions exist on server or CI hosts; missing codecs cause an error. HEIC input is normalized to sRGB without retaining an ICC profile. HDR gain maps are not composited. Use `.rotate()` for orientation tags where the decoder has not already applied them.

## Choosing an output

| Need                                     | Start with                                                             |
| ---------------------------------------- | ---------------------------------------------------------------------- |
| Photo delivery                           | Compare WebP and AVIF at your target size and visual quality           |
| Exact decoded graphics                   | Compare PNG optimization and lossless WebP                             |
| Smaller PNG with acceptable palette loss | `pngQuantize`                                                          |
| Keep JPEG output                         | `compressJpeg` for existing JPEGs; `Transformer.jpeg` after transforms |
| HEIC output on a supported OS            | Check the installed codec and bit-depth limits above                   |

Use the [playground](/playground) to compare common formats, and the [API Reference](/docs/api) for options. Browser display support and this library's codec support are separate considerations.
