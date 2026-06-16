---
title: 'Credits'
description: 'The Rust crates and projects that @napi-rs/image is built on.'
---

# Credits

`@napi-rs/image` is a thin, ergonomic binding layer. Nearly all the real work — decoding, encoding, resampling, optimization — is done by these excellent Rust crates and the C/C++ libraries behind them. Please consider starring and supporting them.

| Project | Role here |
| ------- | --------- |
| [image (image-rs)](https://github.com/image-rs/image) | Core decoding & encoding (PNG, JPEG, WebP, BMP, ICO, TIFF, PNM, TGA, Farbfeld) and the geometric/color transforms (`resize`, `crop`, `blur`, `grayscale`, …). |
| [oxipng](https://github.com/shssoichiro/oxipng) | Lossless PNG optimization — `losslessCompressPng`. |
| [imagequant (libimagequant)](https://github.com/ImageOptim/libimagequant) | Lossy PNG palette quantization — `pngQuantize`. |
| [mozjpeg (mozjpeg-sys)](https://github.com/mozilla/mozjpeg) | JPEG encoding and re-compression — `compressJpeg`. |
| [libwebp (libwebp-sys)](https://chromium.googlesource.com/webm/libwebp) | WebP encode & decode — `webp`, `webpLossless`. |
| [libavif](https://github.com/AOMediaCodec/libavif) + [aom](https://aomedia.googlesource.com/aom/) | AVIF encode & decode — `avif`. |
| [resvg](https://github.com/RazrFalcon/resvg) | SVG rasterization — `Transformer.fromSvg`. |
| [fast_image_resize](https://github.com/Cykooz/fast_image_resize) | SIMD-accelerated resizing — `fastResize`. |
| [rexif](https://github.com/rafalh/rust-rexif) | EXIF metadata parsing — `metadata`, EXIF-aware `rotate`. |

## napi-rs

The Node.js native addon and the cross-origin-isolated WebAssembly build are both produced by [napi-rs](https://napi.rs) — the framework for building pre-compiled Node.js add-ons in Rust. The browser build additionally relies on [emnapi](https://github.com/toyobayashi/emnapi), which implements the Node-API on top of Emscripten/WASI so the same Rust code runs in the browser.

## License

`@napi-rs/image` is [MIT licensed](https://github.com/Brooooooklyn/Image/blob/main/LICENSE). The bundled codecs retain their own upstream licenses.

---

Built and maintained by [LongYinan](https://github.com/Brooooooklyn). If this library saves you time, consider [sponsoring](https://github.com/sponsors/Brooooooklyn).
