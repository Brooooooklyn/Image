// website/pages/playground/protocol.ts
// Numeric mirrors of @napi-rs/image enums (kept here so UI/snippet need no wasm import).
export const ResizeFilter = { Nearest: 0, Triangle: 1, CatmullRom: 2, Gaussian: 3, Lanczos3: 4 } as const
export const ResizeFit = { Cover: 0, Fill: 1, Inside: 2 } as const
export const Chroma = { Yuv444: 0, Yuv422: 1, Yuv420: 2, Yuv400: 3 } as const
export const Orientation = {
  Horizontal: 1, Rotate90Cw: 6, Rotate180: 3, Rotate270Cw: 8,
} as const // the four the UI exposes; 'auto' = use embedded EXIF

export type ConvertFormat = 'webp' | 'webpLossless' | 'avif' | 'jpeg' | 'png'
export type CompressCodec = 'jpeg' | 'pngLossless' | 'pngQuantize'

export type ConvertOp = { kind: 'convert'; format: ConvertFormat; quality: number; chroma: number }
export type CompressOp = { kind: 'compress'; codec: CompressCodec; quality: number; maxQuality: number }
export type TransformOp = {
  kind: 'transform'
  resize: { enabled: boolean; width: number; height: number | null; filter: number; fit: number }
  rotate: number | 'auto' | null // Orientation value, 'auto' (EXIF), or null (none)
  grayscale: boolean
  invert: boolean
  blur: number | null
  encode: { format: ConvertFormat; quality: number }
}
export type MetadataOp = { kind: 'metadata' }
export type Op = ConvertOp | CompressOp | TransformOp | MetadataOp

export type ResultMeta = { width: number; height: number; format: string; orientation?: number }

export type WorkerRequest = { id: number; op: Op; bytes: ArrayBuffer }
export type WorkerOk =
  | { id: number; ok: true; kind: 'metadata'; meta: ResultMeta }
  | { id: number; ok: true; kind: 'convert' | 'compress' | 'transform'; bytes: ArrayBuffer; outFormat: string }
export type WorkerErr = { id: number; ok: false; error: string }
export type WorkerResponse = WorkerOk | WorkerErr

// MIME for the output format (for Blob preview + download). null = not browser-displayable.
export const OUTPUT_MIME: Record<string, string | null> = {
  webp: 'image/webp', webpLossless: 'image/webp', avif: 'image/avif',
  jpeg: 'image/jpeg', png: 'image/png',
  bmp: 'image/bmp', tiff: null, farbfeld: null, pnm: null, ico: 'image/x-icon', tga: null,
}
export const DISPLAYABLE = (fmt: string) => Boolean(OUTPUT_MIME[fmt])
