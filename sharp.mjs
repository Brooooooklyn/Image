// Test sharp behavior

import { readFileSync, writeFileSync } from 'fs'

import sharp from 'sharp'

import { Decoder } from '@napi-rs/image'

// https://github.com/ianare/exif-samples/blob/master/jpg/orientation/portrait_5.jpg
const WITH_EXIF = readFileSync('./with-exif.jpg')
const PNG = readFileSync('./un-optimized.png')

console.time('sharp webp')

const sharpOutputWebp = await sharp(WITH_EXIF)
  .rotate()
  .resize(450 / 2)
  .webp({ quality: 75 })
  .toBuffer()

console.timeEnd('sharp webp')

writeFileSync('output-exif.sharp.webp', sharpOutputWebp)

console.time('@napi-rs/image webp')

const imageOutputWebp = await new Decoder(WITH_EXIF)
  .rotate()
  .resize(450 / 2)
  .webp(75)

console.timeEnd('@napi-rs/image webp')

writeFileSync('output-exif.image.webp', imageOutputWebp)

console.time('sharp avif')

const sharpOutputAvif = await sharp(PNG)
  .rotate()
  .resize(1052 / 2)
  .avif({ quality: 70 })
  .toBuffer()

console.timeEnd('sharp avif')

writeFileSync('output-exif.sharp.avif', sharpOutputAvif)

console.time('@napi-rs/image avif')

const imageOutputAvif = await new Decoder(PNG)
  .rotate()
  .resize(1052 / 2)
  .avif({ quality: 70, speed: 5 })

console.timeEnd('@napi-rs/image avif')

writeFileSync('output-exif.image.avif', imageOutputAvif)
