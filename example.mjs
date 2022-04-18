import { readFileSync, writeFileSync } from 'fs'

import {
  losslessCompressPngSync,
  compressJpegSync,
  pngQuantizeSync,
  Transformer,
  ResizeFilterType,
} from '@napi-rs/image'
import chalk from 'chalk'

const PNG = readFileSync('./un-optimized.png')
const JPEG = readFileSync('./un-optimized.jpg')
// https://github.com/ianare/exif-samples/blob/master/jpg/orientation/portrait_5.jpg
const WITH_EXIF = readFileSync('./with-exif.jpg')

writeFileSync('optimized-lossless.png', losslessCompressPngSync(PNG))

console.info(chalk.green('Lossless compression png done'))

writeFileSync('optimized-lossy.png', pngQuantizeSync(PNG))

console.info(chalk.green('Lossy compression png done'))

writeFileSync('optimized-lossless.jpg', compressJpegSync(readFileSync('./un-optimized.jpg')))

console.info(chalk.green('Lossless compression jpeg done'))

writeFileSync('optimized-lossless.webp', new Transformer(PNG).webpLosslessSync())

console.info(chalk.green('Lossless encoding webp from PNG done'))

writeFileSync('optimized-lossy-jpeg.webp', new Transformer(JPEG).webpSync(90))

console.info(chalk.green('Encoding webp from JPEG done'))

writeFileSync('optimized-lossy.webp', new Transformer(PNG).webpSync(90))

console.info(chalk.green('Encoding webp from PNG done'))

writeFileSync('optimized.avif', new Transformer(PNG).avifSync())

console.info(chalk.green('Encoding avif from PNG done'))

writeFileSync(
  'output-exif.webp',
  await new Transformer(WITH_EXIF)
    .rotate()
    .resize(450 / 2, null, ResizeFilterType.Lanczos3)
    .webp(75),
)

console.info(chalk.green('Encoding webp from JPEG with EXIF done'))
