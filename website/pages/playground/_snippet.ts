// website/pages/playground/_snippet.ts
import type { Op, ConvertFormat } from './protocol'

const FILTER_NAME = ['Nearest', 'Triangle', 'CatmullRom', 'Gaussian', 'Lanczos3']
const CHROMA_NAME = ['Yuv444', 'Yuv422', 'Yuv420', 'Yuv400']
const ORI_NAME: Record<number, string> = { 1: 'Horizontal', 6: 'Rotate90Cw', 3: 'Rotate180', 8: 'Rotate270Cw' }

function encodeCall(format: ConvertFormat, quality: number): string {
  switch (format) {
    case 'webp': return `.webp(${quality})`
    case 'webpLossless': return `.webpLossless()`
    case 'avif': return `.avif({ quality: ${quality} })`
    case 'jpeg': return `.jpeg(${quality})`
    case 'png': return `.png()`
  }
}

export function snippetFor(op: Op): string {
  if (op.kind === 'metadata') return `await new Transformer(input).metadata(true)`
  if (op.kind === 'convert') {
    if (op.format === 'avif')
      return `await new Transformer(input).avif({ quality: ${op.quality}, chromaSubsampling: ChromaSubsampling.${CHROMA_NAME[op.chroma]} })`
    return `await new Transformer(input)${encodeCall(op.format, op.quality)}`
  }
  if (op.kind === 'compress') {
    if (op.codec === 'jpeg') return `await compressJpeg(input, { quality: ${op.quality} })`
    if (op.codec === 'pngLossless') return `await losslessCompressPng(input)`
    return `await pngQuantize(input, { maxQuality: ${op.maxQuality} })`
  }
  // transform
  const parts: string[] = ['new Transformer(input)']
  if (op.rotate === 'auto') parts.push(`.rotate()`)
  else if (typeof op.rotate === 'number') parts.push(`.rotate(Orientation.${ORI_NAME[op.rotate] ?? 'Horizontal'})`)
  if (op.resize.enabled)
    parts.push(`.resize(${op.resize.width}, ${op.resize.height ?? 'null'}, ResizeFilterType.${FILTER_NAME[op.resize.filter]})`)
  if (op.grayscale) parts.push(`.grayscale()`)
  if (op.invert) parts.push(`.invert()`)
  if (op.blur != null) parts.push(`.blur(${op.blur})`)
  parts.push(encodeCall(op.encode.format, op.encode.quality))
  return `await ${parts.join('')}`
}
