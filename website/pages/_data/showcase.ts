import manifest from '../../public/showcase-manifest.json'

export type ShowcaseRow = {
  label: string
  kind: 'Lossless' | 'Lossy'
  before: string
  after: string
  beforeBytes: number
  afterBytes: number
}

const m = manifest as Record<string, number>
const row = (label: string, kind: ShowcaseRow['kind'], before: string, after: string): ShowcaseRow => ({
  label, kind, before: `/${before}`, after: `/${after}`,
  beforeBytes: m[before], afterBytes: m[after],
})

export const showcaseRows: ShowcaseRow[] = [
  row('new Transformer(PNG).webp(75)', 'Lossy', 'img/un-optimized.png', 'img/optimized-lossy-png.webp'),
  row('new Transformer(PNG).avif({ quality: 75 })', 'Lossy', 'img/un-optimized.png', 'img/optimized-lossy-png.avif'),
  row('pngQuantize({ maxQuality: 75 })', 'Lossy', 'img/un-optimized.png', 'img/optimized-lossy.png'),
  row('new Transformer(PNG).avif({ quality: 100 })', 'Lossless', 'img/un-optimized.png', 'img/optimized-lossless-png.avif'),
  row('new Transformer(PNG).webpLossless()', 'Lossless', 'img/un-optimized.png', 'img/optimized-lossless.webp'),
  row('losslessCompressPng()', 'Lossless', 'img/un-optimized.png', 'img/optimized-lossless.png'),
  row('compressJpeg(JPEG, { quality: 75 })', 'Lossy', 'img/un-optimized.jpg', 'img/optimized-lossy.jpg'),
  row('compressJpeg()', 'Lossless', 'img/un-optimized.jpg', 'img/optimized-lossless.jpg'),
]

export const pct = (r: ShowcaseRow) => Math.round((1 - r.afterBytes / r.beforeBytes) * 100)
export const kb = (n: number) => `${Math.round(n / 1024)} KB`

export const filterDemos: { label: string; src: string }[] = [
  { label: 'grayscale', src: '/img/grayscale.manipulated.webp' },
  { label: 'invert', src: '/img/invert.manipulated.webp' },
  { label: 'blur', src: '/img/blur.manipulated.webp' },
  { label: 'huerotate', src: '/img/huerotate.manipulated.webp' },
  { label: 'contrast', src: '/img/contrast.manipulated.webp' },
  { label: 'brighten', src: '/img/brighten.manipulated.webp' },
  { label: 'crop', src: '/img/crop.manipulated.webp' },
]
