import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { bench, describe } from 'vitest'

import { ChromaSubsampling, Transformer } from '@napi-rs/image'

// Representative input images shipped with the repository.
const ROOT = process.cwd()
const JPEG = readFileSync(join(ROOT, 'un-optimized.jpg'))
const PNG = readFileSync(join(ROOT, 'un-optimized.png'))
const WITH_EXIF = readFileSync(join(ROOT, 'with-exif.jpg'))

// The synchronous encoders run on the main thread, which makes them the right
// fit for CodSpeed's CPU simulation: the whole decode/transform/encode pipeline
// is captured in a single, deterministic measurement (no thread pool offload).
//
// Inputs are resized to a bounded size before encoding so each measurement
// stays representative of a real workload while keeping the instrumented run
// time reasonable. AVIF is pinned to a single thread for determinism.
const SIZE = 384

describe('encode', () => {
  bench('jpeg -> webp', () => {
    new Transformer(JPEG).resize(SIZE).webpSync(75)
  })

  bench('jpeg -> avif', () => {
    new Transformer(JPEG)
      .resize(256)
      .avifSync({ quality: 70, speed: 10, threads: 1, chromaSubsampling: ChromaSubsampling.Yuv420 })
  })

  bench('jpeg -> jpeg', () => {
    new Transformer(JPEG).resize(SIZE).jpegSync(75)
  })

  bench('png -> webp', () => {
    new Transformer(PNG).resize(SIZE).webpSync(75)
  })
})

describe('transform', () => {
  bench('resize -> webp', () => {
    new Transformer(JPEG).resize(320).webpSync(75)
  })

  bench('rotate + resize -> webp', () => {
    new Transformer(WITH_EXIF).rotate().resize(225).webpSync(75)
  })
})
