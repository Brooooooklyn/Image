---
title: 'Getting Started'
description: 'Resize, convert and optimize images with Rust-powered native Node.js addons and browser WebAssembly.'
---

# Getting Started

Rust-powered image processing for Node.js: resize, convert and optimize JPEG, PNG, WebP, AVIF and more. Prebuilt native addons handle server and build-script workloads; a WebAssembly build powers browser applications with a Worker and cross-origin isolation.

Use `Transformer` for thumbnails, format conversion, SVG rasterization and compositing. Use the standalone PNG and JPEG optimizers when you want to keep the input format.

## Install

```sh
npm install @napi-rs/image
# or: yarn add @napi-rs/image
# or: pnpm add @napi-rs/image
```

Use a current Node.js LTS release. On supported platforms, npm installs a prebuilt native addon without a local Rust compiler or `node-gyp` build.

## Quick start

Save this as `transform.mjs`, put a JPEG at `input.jpg`, and run `node transform.mjs`:

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

The constructor accepts encoded bytes. Use `Transformer.fromSvg(svg)` for SVG strings or bytes, and `Transformer.fromRgbaPixels(pixels, width, height)` for raw RGBA8 input.

## Keep the same format

Standalone optimizers return new encoded bytes; they do not modify your input file. Output sizes depend on the source and settings.

```js
import { readFile, writeFile } from 'node:fs/promises'
import { compressJpeg, losslessCompressPng, pngQuantize } from '@napi-rs/image'

const png = await readFile('./input.png')
await writeFile('./optimized.png', await losslessCompressPng(png))
await writeFile('./palette.png', await pngQuantize(png, { maxQuality: 75 })) // lossy
await writeFile('./optimized.jpg', await compressJpeg(await readFile('./input.jpg')))
```

`compressJpeg()` uses lossless coefficient optimization by default. Passing `{ quality: 75 }` selects a lossy re-encode. `Transformer.jpeg(75)` encodes an image from any supported input format.

### Async or sync?

Async encoders, optimizers and metadata readers return a `Promise` and run their work on a background thread pool. Prefer them on servers. `*Sync` methods block the calling thread and can be useful in scripts. In the browser, follow the async Worker pattern below.

```js
const webp = await new Transformer(input).webp(80)
const webpSync = new Transformer(input).webpSync(80)
```

## Supported formats

This table describes the published 1.15.0 API. The API processes still images and does not expose animation. Color-type support varies by encoder.

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

GIF, OpenEXR and farbfeld codecs are not enabled in the published build. The declared `farbfeld()` and `farbfeldSync()` methods currently reject with an unsupported-format error. See [Format Guides](/docs/formats#heic-and-heif) for HEIC restrictions.

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

## Browser WebAssembly

The [playground](/playground) is the reference browser integration. It runs images locally in a dedicated Worker and uses the same async API. Browser integration needs more than the Node.js install:

1. Install `@napi-rs/image-wasm32-wasi` and `buffer`. Keep the WASM and main package versions aligned (for example, both `1.15.0`).
2. Use a module Worker. Initialize `globalThis.Buffer` before dynamically importing the image module. Use async image methods so the worker can service nested thread requests.
3. Configure the bundler to emit ES module workers and include the WASI worker and `.wasm` asset. The existing Vite setup aliases `@napi-rs/image` to `@napi-rs/image-wasm32-wasi` and sets `worker: { format: 'es' }`.
4. Serve over HTTPS (or localhost) with cross-origin isolation. The document needs both headers below; verify `self.crossOriginIsolated === true` and that `SharedArrayBuffer` is available.

```http
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

The playground also sends `Cross-Origin-Embedder-Policy: require-corp` and `Cross-Origin-Resource-Policy: same-origin` on its worker/assets responses. Resources loaded across origins need compatible CORS or resource-policy headers. Apply these policies in development, preview and production.

The working setup is in [vite.config.ts](https://github.com/Brooooooklyn/Image/blob/main/website/vite.config.ts), [worker.ts](https://github.com/Brooooooklyn/Image/blob/main/website/pages/playground/worker.ts), [the Worker client](https://github.com/Brooooooklyn/Image/blob/main/website/pages/playground/_engine.ts) and [void.json](https://github.com/Brooooooklyn/Image/blob/main/website/void.json). The Vite middleware also handles a nested WASI-worker URL quirk in development. Copy the complete integration when using this setup, not just the alias. The worker copies output into a regular `ArrayBuffer` before transferring it back, because WASM memory may be shared.

To run the existing integration locally from the repository root:

```sh
yarn install --immutable
yarn workspace website dev
```

Open `http://localhost:5173/playground`, load the sample and run a conversion. HEIC is unavailable in WASM. Hosts without Workers, shared memory or the required isolation policies cannot use this browser setup.

## Where to next

- [API Reference](/docs/api) — methods and options, with links to the full TypeScript declarations.
- [Format Guides](/docs/formats) — codec settings and platform restrictions.
- [Recipes](/docs/recipes) — thumbnails, SVG, compositing, migration and batch optimization.
- [Playground](/playground) — compare output formats on your own image.
