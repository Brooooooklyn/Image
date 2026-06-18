import { promises as fs } from 'fs'

import { createCanvas, GlobalFonts, Image } from '@napi-rs/canvas'
import { losslessCompressPng } from '@napi-rs/image'

const OG_WIDTH = 1200
const OG_HEIGHT = 630
const SVG_PATH = 'public/img/og.svg'

// The OG SVG sets its text in the site's three faces — Space Grotesk (display),
// Inter (body) and JetBrains Mono (labels/code). resvg (inside @napi-rs/canvas) only
// renders glyphs for fonts it can find, so register them before rasterizing. Google
// Fonts hands back static .ttf URLs (resvg can't read the repo's .woff2 builds) when
// asked with a legacy user-agent; we resolve them fresh each build so the links can't
// go stale. If the network is down each face falls back to the SVG's declared
// Arial/sans-serif/monospace stack, so a flaky connection degrades the typeface
// instead of breaking the build.
const FONTS = [
  { family: 'Space Grotesk', spec: 'Space+Grotesk:500,600,700' },
  { family: 'Inter', spec: 'Inter:400,500,600,700' },
  { family: 'JetBrains Mono', spec: 'JetBrains+Mono:400,500,600,700' },
]

// Google Fonts (and the gstatic CDN) occasionally drop the connection mid-handshake,
// so retry a few times before giving up on a face.
async function fetchRetry(url, init, tries = 4) {
  let lastErr
  for (let i = 0; i < tries; i++) {
    try {
      const res = await fetch(url, init)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      return res
    } catch (err) {
      lastErr = err
    }
  }
  throw lastErr
}

async function registerFont({ family, spec }) {
  if (GlobalFonts.families.some((f) => f.family === family)) return
  try {
    const css = await fetchRetry(`https://fonts.googleapis.com/css?family=${spec}`, {
      headers: { 'User-Agent': 'Mozilla/4.0 (compatible)' },
      redirect: 'follow',
    }).then((res) => res.text())
    const urls = [...css.matchAll(/https:\/\/[^)]+\.ttf/g)].map((m) => m[0])
    if (!urls.length) throw new Error('no .ttf urls in css')
    await Promise.all(
      urls.map(async (url) => {
        const ttf = await fetchRetry(url, { redirect: 'follow' }).then((res) => res.arrayBuffer())
        GlobalFonts.register(Buffer.from(ttf), family)
      }),
    )
  } catch (err) {
    console.warn(`[og-image] could not load ${family}, falling back to system fonts:`, err.message)
  }
}

async function registerFonts() {
  await Promise.all(FONTS.map(registerFont))
}

export async function generateOgImage() {
  await registerFonts()

  const svg = await fs.readFile(SVG_PATH)
  const image = new Image()
  image.src = svg

  const canvas = createCanvas(OG_WIDTH, OG_HEIGHT)
  const ctx = canvas.getContext('2d')
  ctx.drawImage(image, 0, 0, OG_WIDTH, OG_HEIGHT)

  await fs.mkdir('public/img', { recursive: true })
  // Lossless, not palette-quantized: the warm gradients band badly when reduced to
  // 256 colors, and an OG image is fetched once by scrapers so size isn't critical.
  await fs.writeFile('public/img/og.png', await losslessCompressPng(await canvas.encode('png')))
}

if (import.meta.url === `file://${process.argv[1]}`) await generateOgImage()
