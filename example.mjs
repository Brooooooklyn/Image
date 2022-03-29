import { readFileSync, writeFileSync } from 'fs'

import {
  losslessCompressPng,
  compressJpeg,
  pngQuantize,
  losslessEncodeWebp,
  encodeAvif,
  encodeWebp,
} from '@napi-rs/image'

const PNG = readFileSync('./un-optimized.png')
const JPEG = readFileSync('./un-optimized.jpg')

writeFileSync('optimized-lossless.png', losslessCompressPng(PNG))

writeFileSync('optimized-lossy.png', pngQuantize(PNG))

writeFileSync('optimized-lossless.jpg', compressJpeg(readFileSync('./un-optimized.jpg')))

writeFileSync('optimized-lossless.webp', losslessEncodeWebp(PNG))

writeFileSync('optimized-lossy-jpeg.webp', encodeWebp(JPEG, 90))

writeFileSync('optimized-lossy.webp', encodeWebp(PNG, 90))

writeFileSync('optimized.avif', encodeAvif(PNG))
