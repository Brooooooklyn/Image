/* eslint-disable */
/* prettier-ignore */

/* auto-generated by NAPI-RS */

const __nodeFs = require('node:fs')
const __nodePath = require('node:path')
const { WASI: __nodeWASI } = require('node:wasi')
const { Worker } = require('node:worker_threads')

const {
  instantiateNapiModuleSync: __emnapiInstantiateNapiModuleSync,
  getDefaultContext: __emnapiGetDefaultContext,
  createOnMessage: __wasmCreateOnMessageForFsProxy,
} = require('@napi-rs/wasm-runtime')

const __rootDir = __nodePath.parse(process.cwd()).root

const __wasi = new __nodeWASI({
  version: 'preview1',
  env: process.env,
  preopens: {
    [__rootDir]: __rootDir,
  }
})

const __emnapiContext = __emnapiGetDefaultContext()

const __sharedMemory = new WebAssembly.Memory({
  initial: 4000,
  maximum: 65536,
  shared: true,
})

let __wasmFilePath = __nodePath.join(__dirname, 'image.wasm32-wasi.wasm')
const __wasmDebugFilePath = __nodePath.join(__dirname, 'image.wasm32-wasi.debug.wasm')

if (__nodeFs.existsSync(__wasmDebugFilePath)) {
  __wasmFilePath = __wasmDebugFilePath
} else if (!__nodeFs.existsSync(__wasmFilePath)) {
  try {
    __wasmFilePath = __nodePath.resolve('@napi-rs/image-wasm32-wasi')
  } catch {
    throw new Error('Cannot find image.wasm32-wasi.wasm file, and @napi-rs/image-wasm32-wasi package is not installed.')
  }
}

const { instance: __napiInstance, module: __wasiModule, napiModule: __napiModule } = __emnapiInstantiateNapiModuleSync(__nodeFs.readFileSync(__wasmFilePath), {
  context: __emnapiContext,
  asyncWorkPoolSize: (function() {
    const threadsSizeFromEnv = Number(process.env.NAPI_RS_ASYNC_WORK_POOL_SIZE ?? process.env.UV_THREADPOOL_SIZE)
    // NaN > 0 is false
    if (threadsSizeFromEnv > 0) {
      return threadsSizeFromEnv
    } else {
      return 4
    }
  })(),
  wasi: __wasi,
  onCreateWorker() {
    const worker = new Worker(__nodePath.join(__dirname, 'wasi-worker.mjs'), {
      env: process.env,
      execArgv: ['--experimental-wasi-unstable-preview1'],
    })
    worker.onmessage = ({ data }) => {
      __wasmCreateOnMessageForFsProxy(__nodeFs)(data)
    }
    return worker
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
  }
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
module.exports.Transformer = __napiModule.exports.Transformer
module.exports.ChromaSubsampling = __napiModule.exports.ChromaSubsampling
module.exports.CompressionType = __napiModule.exports.CompressionType
module.exports.compressJpeg = __napiModule.exports.compressJpeg
module.exports.compressJpegSync = __napiModule.exports.compressJpegSync
module.exports.FastResizeFilter = __napiModule.exports.FastResizeFilter
module.exports.FilterType = __napiModule.exports.FilterType
module.exports.JsColorType = __napiModule.exports.JsColorType
module.exports.losslessCompressPng = __napiModule.exports.losslessCompressPng
module.exports.losslessCompressPngSync = __napiModule.exports.losslessCompressPngSync
module.exports.Orientation = __napiModule.exports.Orientation
module.exports.pngQuantize = __napiModule.exports.pngQuantize
module.exports.pngQuantizeSync = __napiModule.exports.pngQuantizeSync
module.exports.PngRowFilter = __napiModule.exports.PngRowFilter
module.exports.ResizeFilterType = __napiModule.exports.ResizeFilterType
module.exports.ResizeFit = __napiModule.exports.ResizeFit
