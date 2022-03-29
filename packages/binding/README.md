# `@napi-rs/image`

Encode and optimize images library.

See [Examples](../../example.mjs) for usage.

[![install size](https://packagephobia.com/badge?p=@napi-rs/image)](https://packagephobia.com/result?p=@napi-rs/image)
[![Downloads](https://img.shields.io/npm/dm/@napi-rs/image.svg?sanitize=true)](https://npmcharts.com/compare/@napi-rs/image?minimal=true)

## Encode:

This library support the following formats conversation:

- png => webp
- jpeg => webp
- png => avif
- jpeg => avif

### Webp

#### Lossless encode

```js
import { losslessEncodeWebp } from '@napi-rs/image'

/**
 * @param {Buffer} `jpeg` or `png` buffer, throw error if mimetype mismatch
 * @return {Buffer} Encoded lossless `webp` buffer
 */
losslessEncodeWebp(fileBuffer)
```

#### Lossy encode

```js
import { encodeWebp } from '@napi-rs/image'

/**
 * The quality factor `quality_factor` ranges from 0 to 100 and controls the loss and quality during compression.
 * The value 0 corresponds to low quality and small output sizes, whereas 100 is the highest quality and largest output size.
 * https://developers.google.com/speed/webp/docs/api#simple_encoding_api
 *
 * @param {Buffer} `jpeg` or `png` buffer, throw error if mimetype mismatch
 * @return {Buffer} encoded lossy `webp` buffer
 */
encodeWebp(fileBuffer, qualityFactor)
```

### AVIF

```js
import { encodeAvif } from '@napi-rs/image'

/**
 * @param {Buffer} `jpeg` or `png` buffer, throw error if mimetype mismatch
 * @param {AvifConfig | undefined} optional `AVIF` encode config
 * @return {Buffer} encoded `AVIF` buffer
 */
encodeAvif(fileBuffer, options)
```

`AVIF` encode config:

```ts
export interface AvifConfig {
  /** 0-100 scale */
  quality?: number | undefined | null
  /** 0-100 scale */
  alphaQuality?: number | undefined | null
  /** rav1e preset 1 (slow) 10 (fast but crappy) */
  speed?: number | undefined | null
  /** True if RGBA input has already been premultiplied. It inserts appropriate metadata. */
  premultipliedAlpha?: boolean | undefined | null
  /** Which pixel format to use in AVIF file. RGB tends to give larger files. */
  colorSpace?: ColorSpace | undefined | null
  /** How many threads should be used (0 = match core count) */
  threads?: number | undefined | null
}
export const enum ColorSpace {
  YCbCr = 0,
  RGB = 1,
}
```

## Image optimize

### PNG

#### Lossless

```js
import { losslessCompressPng } from '@napi-rs/image'

/**
 * @param {Buffer} un-optimized `png` buffer as input
 * @param {PNGLosslessOptions | undefined} optional optimize options
 * @returns {Buffer} optimized `png` buffer
 */
losslessCompressPng(pngBuffer, options)
```

Optimize options:

```ts
export interface PNGLosslessOptions {
  /**
   * Attempt to fix errors when decoding the input file rather than returning an Err.
   * Default: `false`
   */
  fixErrors?: boolean | undefined | null
  /**
   * Write to output even if there was no improvement in compression.
   * Default: `false`
   */
  force?: boolean | undefined | null
  /** Which filters to try on the file (0-5) */
  filter?: Array<number> | undefined | null
  /**
   * Whether to attempt bit depth reduction
   * Default: `true`
   */
  bitDepthReduction?: boolean | undefined | null
  /**
   * Whether to attempt color type reduction
   * Default: `true`
   */
  colorTypeReduction?: boolean | undefined | null
  /**
   * Whether to attempt palette reduction
   * Default: `true`
   */
  paletteReduction?: boolean | undefined | null
  /**
   * Whether to attempt grayscale reduction
   * Default: `true`
   */
  grayscaleReduction?: boolean | undefined | null
  /**
   * Whether to perform IDAT recoding
   * If any type of reduction is performed, IDAT recoding will be performed regardless of this setting
   * Default: `true`
   */
  idatRecoding?: boolean | undefined | null
  /** Whether to remove ***All non-critical headers*** on PNG */
  strip?: boolean | undefined | null
  /** Whether to use heuristics to pick the best filter and compression */
  useHeuristics?: boolean | undefined | null
}
```

#### Lossy

```js
import { pngQuantize } from '@napi-rs/image'

/**
 * @param {Buffer} un-optimized `png` buffer
 * @param {PNGQuantizeOptions | undefined} optional optimize options
 * @returns {Buffer} optimized `png` buffer
 */
pngQuantize(pngBuffer, options)
```

PNG quantize options:

```ts
export interface PngQuantOptions {
  /** default is 70 */
  minQuality?: number | undefined | null
  /** default is 99 */
  maxQuality?: number | undefined | null
  /**
   * 1- 10
   * Faster speeds generate images of lower quality, but may be useful for real-time generation of images.
   * default: 5
   */
  speed?: number | undefined | null
  /**
   * Number of least significant bits to ignore.
   * Useful for generating palettes for VGA, 15-bit textures, or other retro platforms.
   */
  posterization?: number | undefined | null
}
```

### JPEG

#### Lossless

```js
import { compressJpeg } from '@napi-rs/image'

/**
 * @param {Buffer} un-optimized `jpeg` buffer
 * @param {JpegLosslessOptions | undefined} optional optimize options
 * @returns {Buffer} optimized `jpeg` buffer
 */
compressJpeg(buffer, options)
```

`JPEG` lossless optimize options:

```ts
export interface PNGLosslessOptions {
  /**
   * Attempt to fix errors when decoding the input file rather than returning an Err.
   * Default: `false`
   */
  fixErrors?: boolean | undefined | null
  /**
   * Write to output even if there was no improvement in compression.
   * Default: `false`
   */
  force?: boolean | undefined | null
  /** Which filters to try on the file (0-5) */
  filter?: Array<number> | undefined | null
  /**
   * Whether to attempt bit depth reduction
   * Default: `true`
   */
  bitDepthReduction?: boolean | undefined | null
  /**
   * Whether to attempt color type reduction
   * Default: `true`
   */
  colorTypeReduction?: boolean | undefined | null
  /**
   * Whether to attempt palette reduction
   * Default: `true`
   */
  paletteReduction?: boolean | undefined | null
  /**
   * Whether to attempt grayscale reduction
   * Default: `true`
   */
  grayscaleReduction?: boolean | undefined | null
  /**
   * Whether to perform IDAT recoding
   * If any type of reduction is performed, IDAT recoding will be performed regardless of this setting
   * Default: `true`
   */
  idatRecoding?: boolean | undefined | null
  /** Whether to remove ***All non-critical headers*** on PNG */
  strip?: boolean | undefined | null
  /** Whether to use heuristics to pick the best filter and compression */
  useHeuristics?: boolean | undefined | null
}
```
