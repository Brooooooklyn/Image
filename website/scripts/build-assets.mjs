import { generateDemoImages } from './generate-img.mjs'
import { generateOgImage } from './og-image.mjs'
import { generateChangelog } from './changelog.mjs'
import { generateShowcaseManifest } from './showcase-manifest.mjs'

// The network-free subset the dev server needs to render the landing page on a fresh
// clone: the demo images the showcase/filter sections display, plus the manifest that
// pages/index.tsx statically imports (a missing manifest 500s the page). Neither step
// touches the network, so a flaky connection can't abort `vite dev` or the Playwright
// e2e. The OG image (fetches a font from GitHub) and changelog refresh (GitHub releases
// API) are intentionally excluded — dev's /changelog renders from the committed
// pages/changelog/index.md and og.png is only referenced as a <head> meta URL.
export async function generateDevAssets() {
  await generateDemoImages()
  await generateShowcaseManifest()
}

// The full pass, including the two network-dependent steps. Runs for `vite build`
// and `void deploy`, which want a fresh OG image and changelog.
export async function generateAssets() {
  await generateDemoImages()
  await generateOgImage()
  await generateChangelog()
  await generateShowcaseManifest()
}

if (import.meta.url === `file://${process.argv[1]}`) await generateAssets()
