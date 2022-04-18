# `@napi-rs/image`

Encode and optimize images library.

See [Examples](../../example.mjs) for usage.

[![install size](https://packagephobia.com/badge?p=@napi-rs/image)](https://packagephobia.com/result?p=@napi-rs/image)
[![Downloads](https://img.shields.io/npm/dm/@napi-rs/image.svg?sanitize=true)](https://npmcharts.com/compare/@napi-rs/image?minimal=true)

## Transformer:

This library support encode/decode these formats:

| Format    | Input                                     | Output                                  |
| --------- | ----------------------------------------- | --------------------------------------- |
| RawPixels | RGBA 8 bits pixels                        |                                         |
| JPEG      | Baseline and progressive                  | Baseline JPEG                           |
| PNG       | All supported color types                 | Same as decoding                        |
| BMP       | ✅                                        | Rgb8, Rgba8, Gray8, GrayA8              |
| ICO       | ✅                                        | ✅                                      |
| TIFF      | Baseline(no fax support) + LZW + PackBits | Rgb8, Rgba8, Gray8                      |
| WebP      | No                                        | ✅                                      |
| AVIF      | No                                        | ✅                                      |
| PNM       | PBM, PGM, PPM, standard PAM               | ✅                                      |
| DDS       | DXT1, DXT3, DXT5                          | No                                      |
| TGA       | ✅                                        | Rgb8, Rgba8, Bgr8, Bgra8, Gray8, GrayA8 |
| OpenEXR   | Rgb32F, Rgba32F (no dwa compression)      | Rgb32F, Rgba32F (no dwa compression)    |
| farbfeld  | ✅                                        | ✅                                      |

See [index.d.ts](./index.d.ts) for API reference.
