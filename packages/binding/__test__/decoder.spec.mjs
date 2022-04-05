import { promises as fs } from 'fs'
import { join } from 'path'
import { fileURLToPath } from 'url'

import test from 'ava'

import { Decoder } from '../index.js'

const ROOT_DIR = join(fileURLToPath(import.meta.url), '..', '..', '..', '..')

const PNG = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))
const JPEG = await fs.readFile(join(ROOT_DIR, 'un-optimized.jpg'))
const WITH_EXIF_JPG = await fs.readFile(join(ROOT_DIR, 'with-exif.jpg'))

test('should be able to get metadata from png', async (t) => {
  const decoder = new Decoder(PNG)
  const metadata = await decoder.metadata()
  t.is(metadata.width, 1052)
  t.is(metadata.height, 744)
})

test('should be able to get metadata from jpg', async (t) => {
  const decoder = new Decoder(JPEG)
  const metadata = await decoder.metadata()
  t.is(metadata.width, 1024)
  t.is(metadata.height, 678)
})

test('should be able to get exif from jpg', async (t) => {
  const decoder = new Decoder(WITH_EXIF_JPG)
  const metadata = await decoder.metadata(true)
  t.snapshot(metadata.exif)
  t.is(metadata.orientation, 5)
  t.is(metadata.format, 'jpeg')
})

test('should be able to encode into webp', async (t) => {
  const decoder = new Decoder(PNG)
  await t.notThrowsAsync(() => decoder.webp(75))
})
