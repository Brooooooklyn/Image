import { generateDemoImages } from './generate-img.mjs'
import { generateOgImage } from './og-image.mjs'
import { generateChangelog } from './changelog.mjs'
import { generateShowcaseManifest } from './showcase-manifest.mjs'

export async function generateAssets() {
  await generateDemoImages()
  await generateOgImage()
  await generateChangelog()
  await generateShowcaseManifest()
}

if (import.meta.url === `file://${process.argv[1]}`) await generateAssets()
