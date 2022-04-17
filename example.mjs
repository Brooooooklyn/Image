import { readFileSync, writeFileSync } from 'fs'

import { losslessCompressPng, compressJpegSync, pngQuantize, Transformer } from '@napi-rs/image'

const PNG = readFileSync('./un-optimized.png')
const JPEG = readFileSync('./un-optimized.jpg')
// https://github.com/ianare/exif-samples/blob/master/jpg/orientation/portrait_5.jpg
const WITH_EXIF = readFileSync('./with-exif.jpg')

writeFileSync('optimized-lossless.png', losslessCompressPng(PNG))

writeFileSync('optimized-lossy.png', pngQuantize(PNG))

writeFileSync('optimized-lossless.jpg', compressJpegSync(readFileSync('./un-optimized.jpg')))

writeFileSync('optimized-lossless.webp', new Transformer(PNG).webpLosslessSync())

writeFileSync('optimized-lossy-jpeg.webp', new Transformer(JPEG).webpSync(90))

writeFileSync('optimized-lossy.webp', new Transformer(PNG).webpSync(90))

writeFileSync('optimized.avif', new Transformer(PNG).avifSync())

writeFileSync(
  'output-exif.webp',
  await new Transformer(WITH_EXIF)
    .rotate()
    .resize(450 / 2)
    .webp(75),
)
