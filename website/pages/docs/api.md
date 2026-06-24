---
title: 'API Reference'
description: 'Every class, method, function and option in @napi-rs/image.'
---

# API Reference

Everything is exported from the package root:

```ts
import {
  Transformer,
  losslessCompressPng,
  pngQuantize,
  compressJpeg,
  ChromaSubsampling,
  ResizeFilterType,
  ResizeFit,
  // …enums and option types
} from '@napi-rs/image'
```

Every async method also accepts a trailing `AbortSignal` to cancel in-flight work (omitted from signatures below for brevity).

## `Transformer`

Decode an image, optionally transform it, then encode to any supported format.

### Construction

```ts
new Transformer(input: Uint8Array)

// CSS3 colors are accepted for the SVG background, e.g. 'rgba(255,255,255,.8)'
Transformer.fromSvg(input: string | Uint8Array, background?: string | null): Transformer

Transformer.fromRgbaPixels(input: Uint8Array | Uint8ClampedArray, width: number, height: number): Transformer
```

### Metadata

```ts
metadata(withExif?: boolean | null): Promise<Metadata>
metadataSync(withExif?: boolean | null): Metadata
```

`Metadata`:

| Field         | Type                     | Notes                                         |
| ------------- | ------------------------ | --------------------------------------------- |
| `width`       | `number`                 |                                               |
| `height`      | `number`                 |                                               |
| `format`      | `string`                 | e.g. `'jpeg'`, `'png'`, `'webp'`, `'avif'`    |
| `colorType`   | `JsColorType`            | see enum below                                |
| `orientation` | `number?`                | EXIF orientation tag (1–8), if present        |
| `exif`        | `Record<string,string>?` | only when `withExif` is `true`                |

### Encoders

All encoders return a `Promise<Buffer>` (async) or `Buffer` (sync). The async variants run on a background thread pool.

```ts
webp(qualityFactor?: number | null): Promise<Buffer>   // 0–100; lower = smaller
webpSync(qualityFactor?: number | null): Buffer
webpLossless(): Promise<Buffer>
webpLosslessSync(): Buffer

avif(options?: AvifConfig | null): Promise<Buffer>
avifSync(options?: AvifConfig | null): Buffer

png(options?: PngEncodeOptions | null): Promise<Buffer>
pngSync(options?: PngEncodeOptions | null): Buffer

jpeg(quality?: number | null): Promise<Buffer>         // default 90
jpegSync(quality?: number | null): Buffer

// also: bmp, ico, tiff, pnm, tga, farbfeld — each with a *Sync variant, no options
```

```ts
// raw pixels, native-endian byte slice
rawPixels(): Promise<Buffer>
rawPixelsSync(): Buffer
```

### Transforms

Every transform mutates the pipeline and returns `this`, so they chain. They are applied in call order, then the encoder runs.

| Method                                              | Effect                                                                                   |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `rotate(orientation?: Orientation \| null)`         | Apply EXIF orientation. Passing an `Orientation` overrides the embedded EXIF value.      |
| `resize(widthOrOptions, height?, filter?, fit?)`    | High-quality resize. See [`ResizeFilterType`](#resizefiltertype) / [`ResizeFit`](#resizefit). |
| `fastResize(options: FastResizeOptions)`            | Faster SIMD resize with a different filter set ([`FastResizeFilter`](#fastresizefilter)). |
| `crop(x, y, width, height)`                         | Cut out the bounding rectangle.                                                          |
| `overlay(onTop: Uint8Array, x, y)`                  | Composite another encoded image at `(x, y)`.                                             |
| `blur(sigma)`                                       | Gaussian blur; `sigma` controls the amount.                                              |
| `unsharpen(sigma, threshold)`                       | Unsharp mask sharpening.                                                                 |
| `filter3x3(kernel: number[])`                       | Convolve with a 3×3 kernel (9 values).                                                   |
| `grayscale()`                                       | Convert to grayscale.                                                                    |
| `invert()`                                          | Invert all colors.                                                                       |
| `adjustContrast(contrast)`                          | + increases contrast, − decreases.                                                       |
| `brighten(brightness)`                              | + brightens, − darkens.                                                                  |
| `huerotate(hue)`                                    | Rotate hue by degrees (0/360 are no-ops), like CSS `hue-rotate()`.                       |
| `opacity(factor)`                                   | Multiply the alpha channel by `factor` (0–1), like CSS `opacity`. Keeps the bit depth.   |

`resize` signature:

```ts
resize(
  widthOrOptions: number | ResizeOptions,
  height?: number | null,
  filter?: ResizeFilterType | null,
  fit?: ResizeFit | null,
): this
```

## Standalone optimizers

Compress encoded bytes and get smaller encoded bytes of the **same** format. Async variants take any `Uint8Array`; some sync variants are typed for `Buffer`.

```ts
losslessCompressPng(input: Uint8Array, options?: PNGLosslessOptions | null): Promise<Buffer>
losslessCompressPngSync(input: Buffer, options?: PNGLosslessOptions | null): Buffer

pngQuantize(input: Uint8Array, options?: PngQuantOptions | null): Promise<Buffer>
pngQuantizeSync(input: Uint8Array, options?: PngQuantOptions | null): Buffer

compressJpeg(input: Uint8Array, options?: JpegCompressOptions | null): Promise<Buffer>
compressJpegSync(input: Uint8Array, options?: JpegCompressOptions | null): Buffer
```

## Option types

### `AvifConfig`

| Field               | Type                | Default   | Notes                                            |
| ------------------- | ------------------- | --------- | ------------------------------------------------ |
| `quality`           | `number`            | —         | 0–100, 100 = lossless                            |
| `alphaQuality`      | `number`            | —         | 0–100                                            |
| `speed`             | `number`            | `4`       | 1 (slow, best) … 10 (fast, worst)                |
| `threads`           | `number`            | core count| `0` = match CPU cores                            |
| `chromaSubsampling` | `ChromaSubsampling` | `Yuv444`  | `Yuv420` (`'4:2:0'`) for much smaller files      |

### `JpegCompressOptions` (for `compressJpeg`)

| Field           | Type      | Default | Notes                                                |
| --------------- | --------- | ------- | ---------------------------------------------------- |
| `quality`       | `number`  | `100`   | 100 = lossless re-compress (note: differs from `Transformer.jpeg`, which defaults to 90) |
| `optimizeScans` | `boolean` | `true`  | MozJPEG scan optimization → smaller progressive files |

### `PngEncodeOptions` (for `Transformer.png`)

| Field             | Type              | Default          |
| ----------------- | ----------------- | ---------------- |
| `compressionType` | `CompressionType` | `Default`        |
| `filterType`      | `FilterType`      | `NoFilter`       |

### `PNGLosslessOptions` (for `losslessCompressPng`)

| Field                 | Type               | Default | Notes                                          |
| --------------------- | ------------------ | ------- | ---------------------------------------------- |
| `fixErrors`           | `boolean`          | `false` | Try to repair a malformed input instead of erroring |
| `force`               | `boolean`          | `false` | Write output even with no size improvement     |
| `filter`              | `PngRowFilter[]`   | —       | Which row filters to try                       |
| `bitDepthReduction`   | `boolean`          | `true`  |                                                |
| `colorTypeReduction`  | `boolean`          | `true`  |                                                |
| `paletteReduction`    | `boolean`          | `true`  |                                                |
| `grayscaleReduction`  | `boolean`          | `true`  |                                                |
| `idatRecoding`        | `boolean`          | `true`  | Forced on if any reduction runs                |
| `strip`               | `boolean`          | `false` | Remove all non-critical chunks                 |

### `PngQuantOptions` (for `pngQuantize`)

| Field           | Type     | Default | Notes                                          |
| --------------- | -------- | ------- | ---------------------------------------------- |
| `minQuality`    | `number` | `70`    | 0–100                                          |
| `maxQuality`    | `number` | `99`    | 0–100                                          |
| `speed`         | `number` | `5`     | 1–10, higher = faster but lower quality        |
| `posterization` | `number` | —       | Least-significant bits to drop (retro palettes) |

### `ResizeOptions` / `FastResizeOptions`

```ts
interface ResizeOptions     { width: number; height?: number; filter?: ResizeFilterType; fit?: ResizeFit }
interface FastResizeOptions { width: number; height?: number; filter?: FastResizeFilter; fit?: ResizeFit }
```

## Enums

### `ChromaSubsampling`

`Yuv444` (0, no subsampling) · `Yuv422` (1) · `Yuv420` (2, common for the web) · `Yuv400` (3, grayscale).

### `ResizeFilterType`

`Nearest` (0) · `Triangle` (1) · `CatmullRom` (2) · `Gaussian` (3) · `Lanczos3` (4, highest quality).

### `FastResizeFilter`

`Box` (0) · `Bilinear` (1) · `Hamming` (2) · `CatmullRom` (3) · `Mitchell` (4) · `Lanczos3` (5).

### `ResizeFit`

`Cover` (0, default — preserve aspect, crop to fill) · `Fill` (1, stretch) · `Inside` (2, preserve aspect, fit within).

### `Orientation`

EXIF orientation values 1–8: `Horizontal` (1) · `MirrorHorizontal` (2) · `Rotate180` (3) · `MirrorVertical` (4) · `MirrorHorizontalAndRotate270Cw` (5) · `Rotate90Cw` (6) · `MirrorHorizontalAndRotate90Cw` (7) · `Rotate270Cw` (8).

### `CompressionType`

`Default` (0) · `Fast` (1) · `Best` (2).

### `FilterType` (PNG encode)

`NoFilter` (0) · `Sub` (1) · `Up` (2) · `Avg` (3) · `Paeth` (4) · `Adaptive` (5).

### `PngRowFilter` (oxipng)

`None` (0) · `Sub` (1) · `Up` (2) · `Average` (3) · `Paeth` (4).

### `JsColorType`

`L8` (0) · `La8` (1) · `Rgb8` (2) · `Rgba8` (3) · `L16` (4) · `La16` (5) · `Rgb16` (6) · `Rgba16` (7) · `Rgb32F` (8) · `Rgba32F` (9).
