import {
  createOnMessage as __wasmCreateOnMessageForFsProxy,
  getDefaultContext as __emnapiGetDefaultContext,
  instantiateNapiModule as __emnapiInstantiateNapiModule,
  WASI as __WASI,
} from '@napi-rs/wasm-runtime'



const __wasi = new __WASI({
  version: 'preview1',
})

const __wasmUrl = new URL('./image.wasm32-wasi.wasm', import.meta.url).href
const __emnapiContext = __emnapiGetDefaultContext()


const __sharedMemory = new WebAssembly.Memory({
  initial: 4000,
  maximum: 65536,
  shared: true,
})

const __wasmFile = await fetch(__wasmUrl).then((res) => res.arrayBuffer())

const {
  instance: __napiInstance,
  module: __wasiModule,
  napiModule: __napiModule,
} = await __emnapiInstantiateNapiModule(__wasmFile, {
  context: __emnapiContext,
  asyncWorkPoolSize: 4,
  wasi: __wasi,
  onCreateWorker() {
    const worker = new Worker(new URL('./wasi-worker-browser.mjs', import.meta.url), {
      type: 'module',
    })

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
    for (const name of Object.keys(instance.exports)) {
      if (name.startsWith('__napi_register__')) {
        instance.exports[name]()
      }
    }
  },
})
export default __napiModule.exports
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
