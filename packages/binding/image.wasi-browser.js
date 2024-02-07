import {
  instantiateNapiModuleSync as __emnapiInstantiateNapiModuleSync,
  getDefaultContext as __emnapiGetDefaultContext,
  WASI as __WASI,
} from '@napi-rs/wasm-runtime'
import { Volume as __Volume, createFsFromVolume as __createFsFromVolume } from '@napi-rs/wasm-runtime/fs'

import __wasmUrl from './image.wasm32-wasi.wasm?url'

const __fs = __createFsFromVolume(
  __Volume.fromJSON({
    '/': null,
  }),
)

const __wasi = new __WASI({
  version: 'preview1',
  fs: __fs,
})

const __emnapiContext = __emnapiGetDefaultContext()

const __sharedMemory = new WebAssembly.Memory({
  initial: 1024,
  maximum: 10240,
  shared: true,
})

const __wasmFile = await fetch(__wasmUrl).then((res) => res.arrayBuffer())

const {
  instance: __napiInstance,
  module: __wasiModule,
  napiModule: __napiModule,
} = __emnapiInstantiateNapiModuleSync(__wasmFile, {
  context: __emnapiContext,
  asyncWorkPoolSize: 4,
  wasi: __wasi,
  onCreateWorker() {
    return new Worker(new URL('./wasi-worker-browser.mjs', import.meta.url), {
      type: 'module',
    })
  },
  overwriteImports(importObject) {
    importObject.env = {
      ...importObject.env,
      ...importObject.napi,
      ...importObject.emnapi,
      memory: __sharedMemory,
    }
    return importObject
  },
  beforeInit({ instance }) {
    __napi_rs_initialize_modules(instance)
  },
})

function __napi_rs_initialize_modules(__napiInstance) {
  __napiInstance.exports['__napi_register__AvifConfig_struct_0']?.()
  __napiInstance.exports['__napi_register__ChromaSubsampling_1']?.()
  __napiInstance.exports['__napi_register__FastResizeFilter_2']?.()
  __napiInstance.exports['__napi_register__ResizeFit_3']?.()
  __napiInstance.exports['__napi_register__FastResizeOptions_struct_4']?.()
  __napiInstance.exports['__napi_register__JpegCompressOptions_struct_5']?.()
  __napiInstance.exports['__napi_register__compress_jpeg_sync_6']?.()
  __napiInstance.exports['__napi_register__CompressJpegTask_impl_7']?.()
  __napiInstance.exports['__napi_register__compress_jpeg_8']?.()
  __napiInstance.exports['__napi_register__CompressionType_9']?.()
  __napiInstance.exports['__napi_register__FilterType_10']?.()
  __napiInstance.exports['__napi_register__PngEncodeOptions_struct_11']?.()
  __napiInstance.exports['__napi_register__PngRowFilter_12']?.()
  __napiInstance.exports['__napi_register__PNGLosslessOptions_struct_13']?.()
  __napiInstance.exports['__napi_register__lossless_compress_png_sync_14']?.()
  __napiInstance.exports['__napi_register__LosslessPngTask_impl_15']?.()
  __napiInstance.exports['__napi_register__lossless_compress_png_16']?.()
  __napiInstance.exports['__napi_register__PngQuantOptions_struct_17']?.()
  __napiInstance.exports['__napi_register__png_quantize_sync_18']?.()
  __napiInstance.exports['__napi_register__PngQuantTask_impl_19']?.()
  __napiInstance.exports['__napi_register__png_quantize_20']?.()
  __napiInstance.exports['__napi_register__Orientation_21']?.()
  __napiInstance.exports['__napi_register__ResizeFilterType_22']?.()
  __napiInstance.exports['__napi_register__JsColorType_23']?.()
  __napiInstance.exports['__napi_register__Metadata_struct_24']?.()
  __napiInstance.exports['__napi_register__MetadataTask_impl_25']?.()
  __napiInstance.exports['__napi_register__ResizeOptions_struct_26']?.()
  __napiInstance.exports['__napi_register__EncodeTask_impl_27']?.()
  __napiInstance.exports['__napi_register__Transformer_struct_28']?.()
  __napiInstance.exports['__napi_register__Transformer_impl_70']?.()
}
export const Transformer = __napiModule.exports.Transformer
export const ChromaSubsampling = __napiModule.exports.ChromaSubsampling
export const CompressionType = __napiModule.exports.CompressionType
export const compressJpeg = __napiModule.exports.compressJpeg
export const compressJpegSync = __napiModule.exports.compressJpegSync
export const FastResizeFilter = __napiModule.exports.FastResizeFilter
export const FilterType = __napiModule.exports.FilterType
export const JsColorType = __napiModule.exports.JsColorType
export const losslessCompressPng = __napiModule.exports.losslessCompressPng
export const losslessCompressPngSync = __napiModule.exports.losslessCompressPngSync
export const Orientation = __napiModule.exports.Orientation
export const pngQuantize = __napiModule.exports.pngQuantize
export const pngQuantizeSync = __napiModule.exports.pngQuantizeSync
export const PngRowFilter = __napiModule.exports.PngRowFilter
export const ResizeFilterType = __napiModule.exports.ResizeFilterType
export const ResizeFit = __napiModule.exports.ResizeFit
