// Test sharp behavior

import { readFileSync, writeFileSync } from 'fs'

import sharp from 'sharp'

import { ChromaSubsampling, Transformer } from '@napi-rs/image'

// https://github.com/ianare/exif-samples/blob/master/jpg/orientation/portrait_5.jpg
const WITH_EXIF = readFileSync('./with-exif.jpg')

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
