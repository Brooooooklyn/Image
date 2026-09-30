export type Bench = { suite: string; napi: number; sharp: number }
export const benchDefault: Bench[] = [
  { suite: 'WebP', napi: 202, sharp: 169 },
  { suite: 'AVIF', napi: 26, sharp: 24 },
]
export const benchThreadpool: Bench[] = [
  { suite: 'WebP', napi: 431, sharp: 238 },
  { suite: 'AVIF', napi: 36, sharp: 32 },
]
export const benchCaption =
  'Historical single-JPEG results · Apple M1 Max · macOS 12.3.1. Pipeline: EXIF rotate → resize(225) → WebP or AVIF. Node.js and package versions were not recorded.'
