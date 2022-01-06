# `@napi-rs/image`

Image processing library.

![CI](https://github.com/Brooooooklyn/image/workflows/CI/badge.svg)
[![install size](https://packagephobia.com/badge?p=@napi-rs/image)](https://packagephobia.com/result?p=@napi-rs/image)
[![Downloads](https://img.shields.io/npm/dm/@napi-rs/image.svg?sanitize=true)](https://npmcharts.com/compare/@napi-rs/image?minimal=true)

## Support matrix

|                       | node10 | node12 | node14 | node16 | node17 |
| --------------------- | ------ | ------ | ------ | ------ | ------ |
| Windows x64           | ✓      | ✓      | ✓      | ✓      | ✓      |
| Windows x32           | ✓      | ✓      | ✓      | ✓      | ✓      |
| macOS x64             | ✓      | ✓      | ✓      | ✓      | ✓      |
| macOS arm64 (m chips) | ✓      | ✓      | ✓      | ✓      | ✓      |
| Linux x64 gnu         | ✓      | ✓      | ✓      | ✓      | ✓      |
| Linux x64 musl        | ✓      | ✓      | ✓      | ✓      | ✓      |
| Linux arm gnu         | ✓      | ✓      | ✓      | ✓      | ✓      |
| Linux arm64 gnu       | ✓      | ✓      | ✓      | ✓      | ✓      |
| Linux arm64 musl      | ✓      | ✓      | ✓      | ✓      | ✓      |
| Android arm64         | ✓      | ✓      | ✓      | ✓      | ✓      |
| Android armv7         | ✓      | ✓      | ✓      | ✓      | ✓      |
| FreeBSD x64           | ✓      | ✓      | ✓      | ✓      | ✓      |

## Lossless compression

### `PNG`

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
export function losslessCompressPng(input: Buffer, options?: PNGLosslessOptions | undefined | null): Buffer
```

### `JPEG`

```ts
export interface JpegCompressOptions {
  /** Output quality, default is 100 (lossless) */
  quality?: number | undefined | null
  /**
   * If true, it will use MozJPEG’s scan optimization. Makes progressive image files smaller.
   * Default is `true`
   */
  optimizeScans?: boolean | undefined | null
}
export function compressJpeg(input: Buffer, options?: JpegCompressOptions | undefined | null): Buffer
```
