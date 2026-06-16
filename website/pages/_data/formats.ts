export type Support = 'yes' | 'no'
export type FormatRow = { format: string; decode: Support; encode: Support; note?: string }
export const formatRows: FormatRow[] = [
  { format: 'JPEG', decode: 'yes', encode: 'yes' },
  { format: 'PNG', decode: 'yes', encode: 'yes' },
  { format: 'WebP', decode: 'yes', encode: 'yes' },
  { format: 'AVIF', decode: 'yes', encode: 'yes' },
  { format: 'TIFF', decode: 'yes', encode: 'yes' },
  { format: 'BMP', decode: 'yes', encode: 'yes' },
  { format: 'ICO', decode: 'yes', encode: 'yes' },
  { format: 'TGA', decode: 'yes', encode: 'yes' },
  { format: 'PNM', decode: 'yes', encode: 'yes' },
  { format: 'farbfeld', decode: 'yes', encode: 'yes' },
  { format: 'RawPixels (RGBA8)', decode: 'yes', encode: 'yes' },
  { format: 'SVG', decode: 'yes', encode: 'no', note: 'input only' },
  { format: 'DDS (DXT1/3/5)', decode: 'yes', encode: 'no', note: 'decode only' },
  { format: 'HDR (Radiance)', decode: 'yes', encode: 'no', note: 'decode only' },
]
export const matrixCaption = 'WebP and AVIF are fully bidirectional — decode and encode. HDR and DDS are decode-only.'
