// Test sharp behavior

import { readFileSync, writeFileSync } from 'fs'

import sharp from 'sharp'

import { ChromaSubsampling, Transformer, fastResize, FastResizeFilter, ResizeFilterType } from '@napi-rs/image'

// https://github.com/ianare/exif-samples/blob/master/jpg/orientation/portrait_5.jpg
const WITH_EXIF = readFileSync('./with-exif.jpg')

const NASA = readFileSync('./nasa-4928x3279.png')

console.time('sharp webp')

const sharpOutputWebp = await sharp(WITH_EXIF)
  .rotate()
  .resize(450 / 2)
  .webp({ quality: 75 })
  .toBuffer()

console.timeEnd('sharp webp')

writeFileSync('output-exif.sharp.webp', sharpOutputWebp)

console.time('@napi-rs/image webp')

const imageOutputWebp = await new Transformer(WITH_EXIF)
  .rotate()
  .resize(450 / 2)
  .webp(75)

console.timeEnd('@napi-rs/image webp')

writeFileSync('output-exif.image.webp', imageOutputWebp)

const imageOutputWithoutRotateWebp = await new Transformer(WITH_EXIF).resize(450 / 2).webp(75)

writeFileSync('output-exif.no-rotate.image.webp', imageOutputWithoutRotateWebp)

console.time('sharp avif')

const sharpOutputAvif = await sharp(WITH_EXIF)
  .rotate()
  .resize(450 / 2)
  .avif({ quality: 70, chromaSubsampling: '4:2:0' })
  .toBuffer()

console.timeEnd('sharp avif')

writeFileSync('output-exif.sharp.avif', sharpOutputAvif)

console.time('@napi-rs/image avif')

const imageOutputAvif = await new Transformer(WITH_EXIF)
  .rotate()
  .resize(450 / 2)
  .avif({ quality: 70, chromaSubsampling: ChromaSubsampling.Yuv420 })

console.timeEnd('@napi-rs/image avif')

writeFileSync('output-exif.image.avif', imageOutputAvif)

console.time('sharp resize')

await sharp(NASA)
  .resize(1024, 768, {
    kernel: sharp.kernel.lanczos3,
  })
  .png()
  .toBuffer()

console.timeEnd('sharp resize')

console.time('@napi-rs/image resize')

await new Transformer(NASA).resize(1024, 768, ResizeFilterType.Lanczos3).png()

console.timeEnd('@napi-rs/image resize')

console.time('fast resize')

fastResize(NASA, {
  width: 1024,
  height: 768,
  filter: FastResizeFilter.Lanczos3,
})

console.timeEnd('fast resize')
