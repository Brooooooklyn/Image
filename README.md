# `Image`

Image processing library.

This library support encode/decode these formats:

| Format    | Input                                     | Output                                  |
| --------- | ----------------------------------------- | --------------------------------------- |
| RawPixels | RGBA 8 bits pixels                        |                                         |
| JPEG      | Baseline and progressive                  | Baseline JPEG                           |
| PNG       | All supported color types                 | Same as decoding                        |
| BMP       | ✅                                        | Rgb8, Rgba8, Gray8, GrayA8              |
| ICO       | ✅                                        | ✅                                      |
| TIFF      | Baseline(no fax support) + LZW + PackBits | Rgb8, Rgba8, Gray8                      |
| WebP      | No                                        | ✅                                      |
| AVIF      | No                                        | ✅                                      |
| HEIC      | ✅ (macOS only)                           | ✅ (macOS only)                         |
| PNM       | PBM, PGM, PPM, standard PAM               | ✅                                      |
| DDS       | DXT1, DXT3, DXT5                          | No                                      |
| TGA       | ✅                                        | Rgb8, Rgba8, Bgr8, Bgra8, Gray8, GrayA8 |
| OpenEXR   | Rgb32F, Rgba32F (no dwa compression)      | Rgb32F, Rgba32F (no dwa compression)    |
| farbfeld  | ✅                                        | ✅                                      |
| SVG       | ✅                                        |                                         |

See [index.d.ts](./packages/binding/index.d.ts) for API reference.

![CI](https://github.com/Brooooooklyn/image/workflows/CI/badge.svg)

## HEIC support (macOS only)

HEIC decode **and** encode are **macOS-only**. Both delegate to the operating system's **ImageIO**
framework, which holds the HEVC patent license. This means the package **ships no HEVC/HEIC codec**
and incurs no codec licensing. On non-macOS platforms, HEIC decode and `.heic()` / `.heicSync()`
reject with a clear error.

- **Decode:** reads `.heic` / `.heif` (HEVC-in-HEIF, e.g. iPhone photos). 8-bit sources decode to
  RGBA8; 10-bit sources decode to RGBA16 (precision preserved). Wide-gamut input (e.g. Display-P3)
  is color-matched to **sRGB** — v1 normalizes everything to sRGB and carries no ICC profile. EXIF
  orientation is honored just like JPEG.
- **Encode:** `new Transformer(input).heic({ quality, bitDepth })` / `.heicSync(...)`.
  `quality` is `0-100` (default `80`); `quality: 100` is maximum quality (ImageIO has no
  truly-lossless HEIC mode, so expect a ~1-3/255 residual even on flat color). `bitDepth` is `8` or
  `10` (default follows the source — 16-bit images write 10-bit HEVC Main10).
- **Out of scope (v1):** Apple/ISO HDR **gain-map** reconstruction. The base image is decoded at
  full bit depth, but the auxiliary gain map (the iPhone "HDR look") is not composited. Windows
  support is a future phase.

```js
import { Transformer } from '@napi-rs/image'

// decode HEIC -> JPEG (macOS)
const jpeg = await new Transformer(heicBuffer).jpeg(80)

// encode -> HEIC (macOS)
const heic = await new Transformer(pngBuffer).heic({ quality: 80 })
```

## Performance

System info

```
OS: macOS 12.3.1 21E258 arm64
Host: MacBookPro18,2
Kernel: 21.4.0
Shell: zsh 5.8
CPU: Apple M1 Max
GPU: Apple M1 Max
Memory: 9539MiB / 65536MiB
```

```
node bench/bench.mjs

@napi-rs/image 202 ops/s
sharp 169 ops/s
In webp suite, fastest is @napi-rs/image
@napi-rs/image 26 ops/s
sharp 24 ops/s
In avif suite, fastest is @napi-rs/image
```

```
UV_THREADPOOL_SIZE=10 node bench/bench.mjs

@napi-rs/image 431 ops/s
sharp 238 ops/s
In webp suite, fastest is @napi-rs/image
@napi-rs/image 36 ops/s
sharp 32 ops/s
In avif suite, fastest is @napi-rs/image
```

## `@napi-rs/image`

See [Full documentation for `@napi-rs/image`](./packages/binding/README.md)

### Example

You can clone this repo and run the following command to taste the example below:

- `yarn install`
- `node example.mjs`

| Optimization                                                                                            | Raw                                          | Raw Size | Optimized Size |
| ------------------------------------------------------------------------------------------------------- | -------------------------------------------- | -------- | -------------- |
| `losslessCompressPng()` <br/>**Lossless**                                                               | <img src="./un-optimized.png" width="400" /> | `1.2M`   | `876K`         |
| `pngQuantize({ maxQuality: 75 })` <br/>**Lossy**                                                        | <img src="./un-optimized.png" width="400" /> | `1.2M`   | `244K`         |
| `compressJpeg()` <br/>**Lossless**                                                                      | <img src="./un-optimized.jpg" width="400" /> | `192K`   | `184K`         |
| `compressJpeg(75)` <br/>**Lossy**                                                                       | <img src="./un-optimized.jpg" width="400" /> | `192K`   | `104K`         |
| `new Transformer(PNG).webpLossless()`<br/>**Lossless**                                                  | <img src="./un-optimized.png" width="400" /> | `1.2M`   | `676K`         |
| `new Transformer(PNG).webp(75)`<br/>**Lossy**                                                           | <img src="./un-optimized.png" width="400" /> | `1.2M`   | `84K`          |
| `Transformer(PNG).avif({ quality: 100 })`<br/>**Lossless**                                              | <img src="./un-optimized.png" width="400" /> | `1.2M`   | `584K`         |
| `new Transformer(PNG).avif({ quality: 75, chromaSubsampling: ChromaSubsampling.Yuv420 })`<br/>**Lossy** | <img src="./un-optimized.png" width="400" /> | `1.2M`   | `112K`         |

```js
import { readFileSync, writeFileSync } from 'fs'

import {
  losslessCompressPng,
  compressJpeg,
  pngQuantize,
  Transformer,
  ResizeFilterType,
  ChromaSubsampling,
} from '@napi-rs/image'
import chalk from 'chalk'

const PNG = readFileSync('./un-optimized.png')
const JPEG = readFileSync('./un-optimized.jpg')
// https://github.com/ianare/exif-samples/blob/master/jpg/orientation/portrait_5.jpg
const WITH_EXIF = readFileSync('./with-exif.jpg')
const SVG = readFileSync('./input-debian.svg')

writeFileSync('optimized-lossless.png', await losslessCompressPng(PNG))

console.info(chalk.green('Lossless compression png done'))

writeFileSync(
  'optimized-lossy.png',
  await pngQuantize(PNG, {
    maxQuality: 75,
  }),
)

console.info(chalk.green('Lossy compression png done'))

writeFileSync('optimized-lossless.jpg', await compressJpeg(readFileSync('./un-optimized.jpg')))

console.info(chalk.green('Lossless compression jpeg done'))

writeFileSync('optimized-lossy.jpg', await compressJpeg(readFileSync('./un-optimized.jpg'), { quality: 75 }))

console.info(chalk.green('Lossy compression jpeg done'))

writeFileSync('optimized-lossless.webp', await new Transformer(PNG).webpLossless())

console.info(chalk.green('Lossless encoding webp from PNG done'))

writeFileSync('optimized-lossy-png.webp', await new Transformer(PNG).webp(75))

console.info(chalk.green('Encoding webp from PNG done'))

writeFileSync('optimized-lossless-png.avif', await new Transformer(PNG).avif({ quality: 100 }))

console.info(chalk.green('Lossless encoding avif from PNG done'))

writeFileSync(
  'optimized-lossy-png.avif',
  await new Transformer(PNG).avif({ quality: 75, chromaSubsampling: ChromaSubsampling.Yuv420 }),
)

console.info(chalk.green('Lossy encoding avif from PNG done'))

writeFileSync(
  'output-exif.webp',
  await new Transformer(WITH_EXIF)
    .rotate()
    .resize(450 / 2, null, ResizeFilterType.Lanczos3)
    .webp(75),
)

console.info(chalk.green('Encoding webp from JPEG with EXIF done'))

writeFileSync('output-overlay-png.png', await new Transformer(PNG).overlay(PNG, 200, 200).png())

console.info(chalk.green('Overlay an image done'))

writeFileSync('output-debian.jpeg', await Transformer.fromSvg(SVG, 'rgba(238, 235, 230, .9)').jpeg())

console.info(chalk.green('Encoding jpeg from SVG done'))
```
