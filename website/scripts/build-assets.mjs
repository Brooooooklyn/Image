import { generateDemoImages } from './generate-img.mjs'
import { generateOgImage } from './og-image.mjs'
import { generateChangelog } from './changelog.mjs'

export async function generateAssets() {
  await generateDemoImages()
  await generateOgImage()
  await generateChangelog()
}

if (import.meta.url === `file://${process.argv[1]}`) await generateAssets()
