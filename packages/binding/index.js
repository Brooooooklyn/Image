const { existsSync, readFileSync } = require('fs')
const { join } = require('path')

const { platform, arch } = process

let nativeBinding = null
let localFileExisted = false
let loadError = null

function isMusl() {
  // For Node 10
  if (!process.report || typeof process.report.getReport !== 'function') {
    try {
      return readFileSync('/usr/bin/ldd', 'utf8').includes('musl')
    } catch (e) {
      return false
    }
  } else {
    const { glibcVersionRuntime } = process.report.getReport().header
    return !Boolean(glibcVersionRuntime)
  }
}

switch (platform) {
  case 'android':
    if (arch !== 'arm64') {
      throw new Error(`Unsupported architecture on Android ${arch}`)
    }
    localFileExisted = existsSync(join(__dirname, 'image.android-arm64.node'))
    try {
      if (localFileExisted) {
        nativeBinding = require('./image.android-arm64.node')
      } else {
        nativeBinding = require('@napi-rs/image-android-arm64')
      }
    } catch (e) {
      loadError = e
    }
    break
  case 'win32':
    switch (arch) {
      case 'x64':
        localFileExisted = existsSync(
          join(__dirname, 'image.win32-x64-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./image.win32-x64-msvc.node')
          } else {
            nativeBinding = require('@napi-rs/image-win32-x64-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'ia32':
        localFileExisted = existsSync(
          join(__dirname, 'image.win32-ia32-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./image.win32-ia32-msvc.node')
          } else {
            nativeBinding = require('@napi-rs/image-win32-ia32-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm64':
        localFileExisted = existsSync(
          join(__dirname, 'image.win32-arm64-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./image.win32-arm64-msvc.node')
          } else {
            nativeBinding = require('@napi-rs/image-win32-arm64-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Windows: ${arch}`)
    }
    break
  case 'darwin':
    switch (arch) {
      case 'x64':
        localFileExisted = existsSync(join(__dirname, 'image.darwin-x64.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./image.darwin-x64.node')
          } else {
            nativeBinding = require('@napi-rs/image-darwin-x64')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm64':
        localFileExisted = existsSync(
          join(__dirname, 'image.darwin-arm64.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./image.darwin-arm64.node')
          } else {
            nativeBinding = require('@napi-rs/image-darwin-arm64')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on macOS: ${arch}`)
    }
    break
  case 'freebsd':
    if (arch !== 'x64') {
      throw new Error(`Unsupported architecture on FreeBSD: ${arch}`)
    }
    localFileExisted = existsSync(join(__dirname, 'image.freebsd-x64.node'))
    try {
      if (localFileExisted) {
        nativeBinding = require('./image.freebsd-x64.node')
      } else {
        nativeBinding = require('@napi-rs/image-freebsd-x64')
      }
    } catch (e) {
      loadError = e
    }
    break
  case 'linux':
    switch (arch) {
      case 'x64':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'image.linux-x64-musl.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./image.linux-x64-musl.node')
            } else {
              nativeBinding = require('@napi-rs/image-linux-x64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'image.linux-x64-gnu.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./image.linux-x64-gnu.node')
            } else {
              nativeBinding = require('@napi-rs/image-linux-x64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'arm64':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'image.linux-arm64-musl.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./image.linux-arm64-musl.node')
            } else {
              nativeBinding = require('@napi-rs/image-linux-arm64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'image.linux-arm64-gnu.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./image.linux-arm64-gnu.node')
            } else {
              nativeBinding = require('@napi-rs/image-linux-arm64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'arm':
        localFileExisted = existsSync(
          join(__dirname, 'image.linux-arm-gnueabihf.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./image.linux-arm-gnueabihf.node')
          } else {
            nativeBinding = require('@napi-rs/image-linux-arm-gnueabihf')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Linux: ${arch}`)
    }
    break
  default:
    throw new Error(`Unsupported OS: ${platform}, architecture: ${arch}`)
}

if (!nativeBinding) {
  if (loadError) {
    throw loadError
  }
  throw new Error(`Failed to load native binding`)
}

const { losslessCompressPng, compressJpeg, pngQuantize, Ident, svgMin } = nativeBinding

module.exports.losslessCompressPng = losslessCompressPng
module.exports.compressJpeg = compressJpeg
module.exports.pngQuantize = pngQuantize
module.exports.Ident = Ident
module.exports.svgMin = svgMin
