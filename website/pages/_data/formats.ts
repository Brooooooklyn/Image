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
  { format: 'TGA', decode: 'no', encode: 'yes', note: 'input is not auto-detected' },
  { format: 'PNM', decode: 'yes', encode: 'yes' },
  { format: 'HEIC / HEIF', decode: 'yes', encode: 'yes', note: 'macOS / Windows with OS codec only' },
  { format: 'Raw pixels', decode: 'yes', encode: 'yes', note: 'RGBA8 input; native-endian output' },
  { format: 'SVG', decode: 'yes', encode: 'no', note: 'rasterize with fromSvg()' },
  { format: 'DDS (DXT1/3/5)', decode: 'yes', encode: 'no', note: 'decode only' },
  { format: 'HDR (Radiance)', decode: 'yes', encode: 'no', note: 'decode only' },
]
export const matrixCaption =
  'WebP and AVIF decode and encode. HEIC requires macOS or Windows with an OS codec; it is unavailable in the browser. GIF, OpenEXR and farbfeld are not enabled in the published build.'
