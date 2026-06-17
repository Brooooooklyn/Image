import assert from 'node:assert/strict'
import { snippetFor } from '../pages/playground/_snippet.ts'

// convert webp
assert.match(
  snippetFor({ kind: 'convert', format: 'webp', quality: 75, chroma: 0 }),
  /new Transformer\(input\)\.webp\(75\)/,
)
// avif includes chroma when not default
assert.match(
  snippetFor({ kind: 'convert', format: 'avif', quality: 70, chroma: 2 }),
  /\.avif\(\{ quality: 70, chromaSubsampling: ChromaSubsampling\.Yuv420 \}\)/,
)
// compress png quantize
assert.match(
  snippetFor({ kind: 'compress', codec: 'pngQuantize', quality: 75, maxQuality: 80 }),
  /pngQuantize\(input, \{ maxQuality: 80 \}\)/,
)
// transform: rotate auto + resize + grayscale + encode webp
const t = snippetFor({
  kind: 'transform',
  resize: { enabled: true, width: 800, height: null, filter: 4, fit: 0 },
  rotate: 'auto', grayscale: true, invert: false, blur: null,
  encode: { format: 'webp', quality: 75 },
})
assert.match(t, /\.rotate\(\)/)
assert.match(t, /\.resize\(800, null, ResizeFilterType\.Lanczos3\)/)
assert.match(t, /\.grayscale\(\)/)
assert.match(t, /\.webp\(75\)/)
console.log('snippet.test OK')
