export const heroSample = `import { readFile, writeFile } from 'node:fs/promises'
import {
  Transformer,
  ChromaSubsampling,
} from '@napi-rs/image'

const input = await readFile('./input.jpg')
const webp = await new Transformer(input)
  .rotate()
  .resize(225)
  .webp(75)

const avif = await new Transformer(input)
  .rotate()
  .resize(225)
  .avif({
    quality: 70,
    chromaSubsampling: ChromaSubsampling.Yuv420,
  })

await writeFile('./output.webp', webp)
await writeFile('./output.avif', avif)`

export const fullSample = `import { readFileSync, writeFileSync } from 'node:fs'
import {
  Transformer,
  losslessCompressPng,
  ResizeFilterType,
  ChromaSubsampling,
} from '@napi-rs/image'

const PNG = readFileSync('./input.png')

writeFileSync('out.png', await losslessCompressPng(PNG))
writeFileSync('out.webp', await new Transformer(PNG)
  .resize(800, null, ResizeFilterType.Lanczos3)
  .webp(75))
writeFileSync('out.avif', await new Transformer(PNG)
  .resize(800, null, ResizeFilterType.Lanczos3)
  .avif({ quality: 75, chromaSubsampling: ChromaSubsampling.Yuv420 }))`
