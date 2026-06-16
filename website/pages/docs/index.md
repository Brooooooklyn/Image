---
title: 'Getting Started'
description: 'Fast, native image decoding, encoding, transforming and compression for Node.js and the browser, powered by Rust.'
---

# Getting Started

`@napi-rs/image` is a fast image-processing library for **Node.js** and the **browser**. It wraps battle-tested Rust codecs — [mozjpeg](https://github.com/mozilla/mozjpeg), [oxipng](https://github.com/shssoichiro/oxipng), [libimagequant](https://github.com/ImageOptim/libimagequant), [libwebp](https://chromium.googlesource.com/webm/libwebp), [ravif/aom](https://github.com/kornelski/cavif-rs) and [resvg](https://github.com/RazrFalcon/resvg) — behind one small, ergonomic API.

On Node.js it loads a prebuilt native binary (no compiler, no `node-gyp`). Where a native binary is unavailable it falls back to a WebAssembly build, which is also what powers the in-browser [Playground](/playground).

## Install

```bash
npm install @napi-rs/image
```

```bash
yarn add @napi-rs/image
```

```bash
pnpm add @napi-rs/image
```

Prebuilt binaries ship for macOS (x64 / arm64), Windows (x64 / arm64 / ia32), Linux (gnu + musl, x64 / arm64 / armv7), Android arm64, FreeBSD, and a `wasm32-wasi` build for everything else. The right one is selected automatically at `import` time.

## 60-second example

Decode an image, optimize it, and convert it to a modern format:

```ts
import { readFile, writeFile } from 'node:fs/promises'
import { Transformer, ChromaSubsampling } from '@napi-rs/image'

const input = await readFile('./photo.jpg')

// Re-encode to AVIF at quality 75 with 4:2:0 chroma subsampling.
const avif = await new Transformer(input).avif({
  quality: 75,
  chromaSubsampling: ChromaSubsampling.Yuv420,
})

await writeFile('./photo.avif', avif)
```

Or compress a file *in place*, keeping the same format:

```ts
import { readFile, writeFile } from 'node:fs/promises'
import { compressJpeg, losslessCompressPng } from '@napi-rs/image'

await writeFile('./small.jpg', await compressJpeg(await readFile('./big.jpg'), { quality: 75 }))
await writeFile('./small.png', await losslessCompressPng(await readFile('./big.png')))
```

## The mental model

There are two entry points, and they cover different jobs:

```ts
new Transformer(input)        // decode → transform (resize/rotate/crop) → encode to ANY format
  .resize()
  .rotate()                   // chainable, returns `this`
  .webp()                     // async encoders → Promise<Buffer>
  .webpSync()                 // sync encoders  → Buffer

// Standalone optimizers — same format in → same format out, bytes shrunk IN PLACE:
compressJpeg(bytes)
losslessCompressPng(bytes)
pngQuantize(bytes)
```

- **`Transformer`** is for decoding, geometry/color transforms, and converting between formats. Construct it from encoded bytes (`new Transformer(bytes)`), from SVG (`Transformer.fromSvg`), or from raw RGBA pixels (`Transformer.fromRgbaPixels`).
- **Standalone compressors** (`compressJpeg`, `losslessCompressPng`, `pngQuantize`) take encoded bytes and return smaller encoded bytes of the **same** format. Use them when you just want a smaller file.

### Async or sync?

Every encoder and the metadata reader come in two flavours:

- **Async** (`webp`, `avif`, `png`, `jpeg`, `metadata`, …) run on a background thread pool and return a `Promise`. Prefer these on servers so you never block the event loop. They also accept an optional `AbortSignal`.
- **Sync** (`webpSync`, `avifSync`, `metadataSync`, …) run on the calling thread and return immediately. Handy in scripts and CLIs.

```ts
const buf = await new Transformer(input).webp(80)        // async, off the event loop
const buf2 = new Transformer(input).webpSync(80)         // sync, blocks until done
```

## Supported formats

| Format    | Decode (input) | Encode (output) |
| --------- | :------------: | :-------------: |
| JPEG      |       ✅       |       ✅        |
| PNG       |       ✅       |       ✅        |
| WebP      |       ✅       |       ✅        |
| AVIF      |       ✅       |       ✅        |
| GIF       |       ✅       |       —         |
| TIFF      |       ✅       |       ✅        |
| BMP       |       ✅       |       ✅        |
| ICO       |       ✅       |       ✅        |
| TGA       |       ✅       |       ✅        |
| PNM       |       ✅       |       ✅        |
| Farbfeld  |       ✅       |       ✅        |
| SVG       | ✅ (`fromSvg`) |       —         |

## Where to next

- **[API Reference](/docs/api)** — every method, function and option.
- **[Format Guides](/docs/formats)** — choosing quality, speed and chroma for WebP, AVIF, PNG and JPEG.
- **[Recipes](/docs/recipes)** — EXIF-correct thumbnails, SVG rasterization, watermarking, batch optimization, cancellation.
- **[Playground](/playground)** — run all of this in your browser, no install.
