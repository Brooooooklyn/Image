# `@napi-rs/image`

Rust-powered image processing for Node.js: resize, convert and optimize JPEG, PNG, WebP, AVIF and more. Prebuilt native addons handle server and build-script workloads; a WebAssembly build powers browser applications with a Worker and cross-origin isolation.

**[Documentation](https://image.napi.rs/docs)** · **[Browser playground](https://image.napi.rs/playground)** · **[API reference](https://image.napi.rs/docs/api)** · **[Releases](https://github.com/Brooooooklyn/Image/releases)**

[![npm version](https://img.shields.io/npm/v/@napi-rs/image)](https://www.npmjs.com/package/@napi-rs/image)
[![Downloads](https://img.shields.io/npm/dm/@napi-rs/image.svg)](https://www.npmjs.com/package/@napi-rs/image)
[![CI](https://github.com/Brooooooklyn/Image/actions/workflows/CI.yml/badge.svg)](https://github.com/Brooooooklyn/Image/actions/workflows/CI.yml)

## Install and run

```sh
npm install @napi-rs/image
```

Use a current Node.js LTS release. Save this as `transform.mjs`, put a JPEG at `input.jpg`, and run `node transform.mjs`:

```js
import { readFile, writeFile } from 'node:fs/promises'
import { Transformer, ResizeFilterType } from '@napi-rs/image'

const input = await readFile('./input.jpg')
const output = await new Transformer(input)
  .rotate() // apply EXIF orientation
  .resize(800, null, ResizeFilterType.Lanczos3)
  .webp(80)

await writeFile('./output.webp', output)
```

`Transformer` accepts encoded bytes, chains transforms, and returns a `Buffer` from an encoder. Read and write files with Node.js APIs. Async encoders run on a background thread pool; `*Sync` variants block the calling thread.

## Optimize without changing formats

Use the standalone optimizers for PNG or JPEG input. They return new bytes; they do not modify your source file. Output size depends on the image and settings.

```js
import { readFile, writeFile } from 'node:fs/promises'
import { compressJpeg, losslessCompressPng, pngQuantize } from '@napi-rs/image'

const png = await readFile('./input.png')
const jpeg = await readFile('./input.jpg')

await writeFile('./optimized.png', await losslessCompressPng(png))
await writeFile('./palette.png', await pngQuantize(png, { maxQuality: 75 })) // lossy
await writeFile('./optimized.jpg', await compressJpeg(jpeg)) // lossless coefficient optimization
await writeFile('./smaller.jpg', await compressJpeg(jpeg, { quality: 75 })) // lossy re-encode
```

For SVG rasterization, AVIF settings, compositing and batch processing, see the [recipes](https://image.napi.rs/docs/recipes), [format guides](https://image.napi.rs/docs/formats) and [repository example](https://github.com/Brooooooklyn/Image/blob/main/example.mjs). Complete TypeScript signatures ship with the package in [index.d.ts](https://github.com/Brooooooklyn/Image/blob/main/packages/binding/index.d.ts).

## Format support

The following describes the published 1.15.0 API. It processes still images; it does not expose an animation pipeline. Color-type support varies by encoder.

| Format         | Input                          | Output          | Notes                                                               |
| -------------- | ------------------------------ | --------------- | ------------------------------------------------------------------- |
| JPEG           | Yes                            | Yes             | Baseline/progressive input; baseline output                         |
| PNG            | Yes                            | Yes             | Includes alpha and 16-bit input                                     |
| WebP           | Yes                            | Yes             | Lossy or lossless output                                            |
| AVIF           | Yes                            | Yes             | Decoded to 8-bit pixels                                             |
| HEIC / HEIF    | macOS / Windows                | macOS / Windows | HEVC-in-HEIF; requires an OS codec                                  |
| TIFF           | Yes                            | Yes             | Baseline, LZW and PackBits input; no fax support                    |
| BMP            | Yes                            | Yes             |                                                                     |
| ICO            | Yes                            | Yes             |                                                                     |
| PNM            | Yes                            | Yes             | PBM, PGM, PPM and PAM input                                         |
| TGA            | No                             | Yes             | Input cannot be auto-detected by `Transformer`                      |
| DDS            | Yes                            | No              | DXT1, DXT3 and DXT5 input                                           |
| HDR (Radiance) | Yes                            | No              |                                                                     |
| SVG            | `Transformer.fromSvg()`        | No              | Rasterized before further transforms                                |
| Raw pixels     | `Transformer.fromRgbaPixels()` | `rawPixels()`   | Input: RGBA8; output: native-endian bytes in the image's color type |

GIF, OpenEXR and farbfeld codecs are not enabled in the published build. Although `farbfeld()` / `farbfeldSync()` appear in the API, they currently reject with an unsupported-format error.

## Platforms

Prebuilt native packages are published for:

| Platform     | Architectures                 |
| ------------ | ----------------------------- |
| macOS        | x64, arm64                    |
| Windows      | x64, arm64, ia32 (MSVC)       |
| Linux, glibc | x64, arm64, armv7 (gnueabihf) |
| Linux, musl  | x64, arm64                    |
| Android      | arm64                         |
| FreeBSD      | x64                           |

Keep npm optional dependencies enabled so the matching native package can be installed. Browser applications need the separate `@napi-rs/image-wasm32-wasi` package and the setup below; installing the native package alone does not make every runtime supported.

### HEIC on macOS and Windows

HEIC decoding and encoding use ImageIO on macOS and Windows Imaging Component (WIC) on Windows. The package does not bundle a HEVC codec. HEIC is unavailable on Linux, Android, FreeBSD and in the browser/WASM build.

- **Windows:** the HEIF Image Extensions and HEVC Video Extensions must be available to WIC. Do not assume they are installed on Windows Server or CI hosts; operations fail when the codec is missing. Decoding produces RGBA8, including for 10-bit sources. Encoding produces opaque 8-bit images; alpha is flattened and `bitDepth: 10` is rejected.
- **macOS:** 8-bit sources decode to RGBA8 and 10-bit sources to RGBA16. Encoding supports 8 or 10 bits; by default, 16-bit input selects 10-bit output. Quality 90–100 shares the same encoder ceiling; HEIC encoding has no lossless mode.
- HEIC input is normalized to sRGB without preserving an ICC profile. HDR gain maps are not reconstructed. Use `.rotate()` to apply orientation tags where the OS decoder has not already applied them.

```js
// macOS or Windows with the required codec; input is encoded image bytes.
const jpeg = await new Transformer(heicBytes).rotate().jpeg(80)
const heic = await new Transformer(pngBytes).heic({ quality: 80 })
```

### Browser WebAssembly

The [playground](https://image.napi.rs/playground) processes images locally in a dedicated Worker. Its setup requires:

- `@napi-rs/image-wasm32-wasi`, module Workers and a bundler that emits the nested WASI worker and `.wasm` asset.
- A `Buffer` polyfill initialized before importing the WASM module, and async image methods so the worker can service thread requests.
- A secure context (HTTPS or localhost), `SharedArrayBuffer`, and cross-origin isolation: `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` on the document, with compatible policies on workers and assets.

Follow the [browser setup guide](https://image.napi.rs/docs#browser-webassembly) and the working [Vite configuration](https://github.com/Brooooooklyn/Image/blob/main/website/vite.config.ts), [Worker](https://github.com/Brooooooklyn/Image/blob/main/website/pages/playground/worker.ts) and [response headers](https://github.com/Brooooooklyn/Image/blob/main/website/void.json). HEIC is not available in WASM.

## Performance

These are historical results already recorded in this repository, not a new benchmark of 1.15.0. The [benchmark script](https://github.com/Brooooooklyn/Image/blob/main/bench/bench.mjs) uses one EXIF JPEG (`with-exif.jpg`), applies orientation, resizes to width 225, and encodes WebP (quality 75) or AVIF (quality 70, 4:2:0). Work is submitted concurrently up to the CPU count.

Recorded hardware: **Apple M1 Max**, macOS **12.3.1**, arm64. Node.js and package versions were not recorded with the results.

| Thread pool             | Output | `@napi-rs/image` |   `sharp` |
| ----------------------- | ------ | ---------------: | --------: |
| Default                 | WebP   |        202 ops/s | 169 ops/s |
| Default                 | AVIF   |         26 ops/s |  24 ops/s |
| `UV_THREADPOOL_SIZE=10` | WebP   |        431 ops/s | 238 ops/s |
| `UV_THREADPOOL_SIZE=10` | AVIF   |         36 ops/s |  32 ops/s |

The 1.8× ratio applies only to the recorded WebP pipeline with a thread pool of 10. Equal numeric quality settings do not establish equal visual quality across encoders. Measure your own images, output sizes, quality targets and deployment hardware before choosing a library or concurrency setting.

To repeat the workload from a development checkout with dependencies and a native binding installed:

```sh
node bench/bench.mjs
UV_THREADPOOL_SIZE=10 node bench/bench.mjs
```

## Moving from sharp

Both libraries can express a resize-and-encode pipeline, but their APIs and defaults differ. In `@napi-rs/image`, `new Transformer(bytes)` takes the input, `.webp(80)` returns the output directly, and `.rotate()` applies EXIF orientation. There is no trailing `.toBuffer()` call. See the [migration recipe](https://image.napi.rs/docs/recipes#moving-a-thumbnail-pipeline-from-sharp) for a small example and behavior differences to check.

## License and credits

MIT. See [LICENSE](https://github.com/Brooooooklyn/Image/blob/main/LICENSE) and [codec credits](https://image.napi.rs/docs/credits).
