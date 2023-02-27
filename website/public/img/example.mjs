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

writeFileSync('optimized-lossless.jpg', await compressJpeg(JPEG))

console.info(chalk.green('Lossless compression jpeg done'))

writeFileSync('optimized-lossy.jpg', await compressJpeg(JPEG, { quality: 75 }))

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

writeFileSync(
  'output-overlay-png.png',
  await new Transformer(PNG).overlay(PNG, 200, 200).png()
)

console.info(chalk.green('Overlay an image done'))

writeFileSync("output-debian.jpeg", await Transformer.fromSvg(SVG, 'rgba(238, 235, 230, .9)').jpeg())

console.info(chalk.green('Encoding jpeg from SVG done'))