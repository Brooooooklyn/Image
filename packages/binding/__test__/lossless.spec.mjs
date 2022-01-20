import { promises as fs } from 'fs'
import { join } from 'path'
import { fileURLToPath } from 'url'

import test from 'ava'

import { losslessCompressPng, compressJpeg } from '../index.js'

const ROOT_DIR = join(fileURLToPath(import.meta.url), '..', '..', '..', '..')

const PNG = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))
const JPEG = await fs.readFile(join(ROOT_DIR, 'un-optimized.jpg'))

test('should be able to lossless optimize png image', async (t) => {
  const dest = losslessCompressPng(PNG)
  t.true(dest.length < PNG.length)
})

test('should be able to lossless optimize jpeg image', async (t) => {
  const dest = compressJpeg(JPEG, { quality: 100 })
  t.true(dest.length < PNG.length)
})
