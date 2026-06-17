import { stat, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

const FILES = [
  'img/un-optimized.png', 'img/un-optimized.jpg',
  'img/optimized-lossless.png', 'img/optimized-lossy.png',
  'img/optimized-lossless.jpg', 'img/optimized-lossy.jpg',
  'img/optimized-lossless.webp', 'img/optimized-lossy-png.webp',
  'img/optimized-lossless-png.avif', 'img/optimized-lossy-png.avif',
]

export async function generateShowcaseManifest() {
  const pub = join(process.cwd(), 'public')
  const sizes = {}
  for (const f of FILES) {
    sizes[f] = (await stat(join(pub, f))).size // throws if a demo image is missing — good, fail the build
  }
  await writeFile(join(pub, 'showcase-manifest.json'), JSON.stringify(sizes, null, 2))
}

if (import.meta.url === `file://${process.argv[1]}`) await generateShowcaseManifest()
