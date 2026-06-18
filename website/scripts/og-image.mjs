import { promises as fs } from 'fs'

import { createCanvas, GlobalFonts, Image } from '@napi-rs/canvas'
import { pngQuantize } from '@napi-rs/image'

const OG_WIDTH = 1200
const OG_HEIGHT = 630
const SVG_PATH = 'public/img/og.svg'

// The OG SVG sets its text in Inter. resvg (inside @napi-rs/canvas) only renders
// glyphs for fonts it can find, so register Inter before rasterizing. Google Fonts
// hands back static .ttf URLs (resvg can't read .woff2) when asked with a legacy
// user-agent; we resolve them fresh each build so the link can't go stale. If the
// network is down the SVG falls back to its declared Arial/sans-serif stack, so a
// flaky connection degrades the typeface instead of breaking the build.
async function registerInter() {
  if (GlobalFonts.families.some(({ family }) => family === 'Inter')) return
  try {
    const css = await fetch('https://fonts.googleapis.com/css?family=Inter:400,500,600,700,800', {
      headers: { 'User-Agent': 'Mozilla/4.0 (compatible)' },
      redirect: 'follow',
    }).then((res) => res.text())
    const urls = [...css.matchAll(/https:\/\/[^)]+\.ttf/g)].map((m) => m[0])
    await Promise.all(
      urls.map(async (url) => {
        const ttf = await fetch(url, { redirect: 'follow' }).then((res) => res.arrayBuffer())
        GlobalFonts.register(Buffer.from(ttf), 'Inter')
      }),
    )
  } catch (err) {
    console.warn('[og-image] could not load Inter, falling back to system fonts:', err.message)
  }
}

export async function generateOgImage() {
  await registerInter()

  const svg = await fs.readFile(SVG_PATH)
  const image = new Image()
  image.src = svg

  const canvas = createCanvas(OG_WIDTH, OG_HEIGHT)
  const ctx = canvas.getContext('2d')
  ctx.drawImage(image, 0, 0, OG_WIDTH, OG_HEIGHT)

  await fs.mkdir('public/img', { recursive: true })
  await fs.writeFile(
    'public/img/og.png',
    await pngQuantize(await canvas.encode('png'), {
      maxQuality: 90,
      minQuality: 75,
    }),
  )
}

if (import.meta.url === `file://${process.argv[1]}`) await generateOgImage()
