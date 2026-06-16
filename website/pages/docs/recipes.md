---
title: 'Recipes'
description: 'Practical, copy-paste patterns: thumbnails, SVG, watermarks, batch optimization, cancellation, tuning.'
---

# Recipes

## EXIF-correct thumbnails

Photos from phones store rotation in EXIF, not pixels. Call `rotate()` first so the thumbnail isn't sideways, then resize with a high-quality filter:

```ts
import { Transformer, ResizeFilterType } from '@napi-rs/image'

const thumb = await new Transformer(photo)
  .rotate()                              // bake in EXIF orientation
  .resize(320, null, ResizeFilterType.Lanczos3) // width 320, keep aspect
  .webp(80)
```

Passing no argument to `rotate()` uses the embedded EXIF value; pass an [`Orientation`](/docs/api#orientation) to override it.

## Rasterize an SVG

```ts
import { Transformer } from '@napi-rs/image'

// background accepts any CSS3 color (including alpha)
const png = await Transformer.fromSvg(svgString, 'rgba(255,255,255,1)').png()
```

## From raw RGBA pixels (e.g. blurhash)

```ts
import { Transformer } from '@napi-rs/image'
import { decode } from 'blurhash'

const pixels = decode('LEHV6nWB2yk8pyo0adR*.7kCMdnj', 32, 32) // Uint8ClampedArray
const placeholder = await Transformer.fromRgbaPixels(pixels, 32, 32).webp()
```

## Watermark with `overlay`

```ts
import { Transformer } from '@napi-rs/image'

const watermarked = await new Transformer(base)
  .overlay(logoPngBytes, 24, 24) // composite logo at (24, 24)
  .jpeg(90)
```

## Batch-optimize a folder (bounded concurrency)

The async methods run off the main thread, so you can process many files at once. Cap concurrency so you don't oversubscribe the thread pool:

```ts
import { readdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { losslessCompressPng } from '@napi-rs/image'

async function mapLimit<T>(items: T[], limit: number, fn: (t: T) => Promise<void>) {
  const queue = [...items]
  await Promise.all(
    Array.from({ length: limit }, async () => {
      while (queue.length) await fn(queue.shift()!)
    }),
  )
}

const dir = './images'
const pngs = (await readdir(dir)).filter((f) => f.endsWith('.png'))

await mapLimit(pngs, 8, async (name) => {
  const out = await losslessCompressPng(await readFile(join(dir, name)))
  await writeFile(join(dir, name), out)
})
```

## Cancel in-flight work with `AbortSignal`

Every async method accepts a trailing `AbortSignal` — handy for request timeouts on a server:

```ts
const ac = new AbortController()
const t = setTimeout(() => ac.abort(), 2000)
try {
  const avif = await new Transformer(input).avif({ quality: 60 }, ac.signal)
  return avif
} finally {
  clearTimeout(t)
}
```

## Performance tuning

- **Prefer the async methods** on servers. They run on libuv's thread pool and keep the event loop free.
- **Raise the thread pool** for throughput. The default libuv pool is 4 threads; encoders are CPU-bound, so more threads = more parallel encodes:

  ```bash
  UV_THREADPOOL_SIZE=10 node server.js
  ```

  In benchmarks, lifting the pool from 4 → 10 roughly doubled WebP throughput.
- **Use `*Sync` in CLIs and build scripts** where blocking is fine and the per-call overhead of dispatching to the pool isn't worth it.
- **AVIF `speed`** is the biggest single knob for encode time — raise it while iterating, lower it for final output.

See the [API Reference](/docs/api) for full signatures and the [Format Guides](/docs/formats) for quality/size trade-offs.
