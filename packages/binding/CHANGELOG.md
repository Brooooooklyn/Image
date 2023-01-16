# Change Log

All notable changes to this project will be documented in this file.
See [Conventional Commits](https://conventionalcommits.org) for commit guidelines.

# [1.5.0](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.4.4...@napi-rs/image@1.5.0) (2023-01-16)

### Features

- **image:** implement `overlay` ([#33](https://github.com/Brooooooklyn/Image/issues/33)) ([8bcc5de](https://github.com/Brooooooklyn/Image/commit/8bcc5de9762eb80dd460c4a1f7450c3961b4170c))
- **image:** provide fast resize method ([#34](https://github.com/Brooooooklyn/Image/issues/34)) ([f52fd45](https://github.com/Brooooooklyn/Image/commit/f52fd452456151abb1271404e6f82b6e3fac3618))

### Performance Improvements

- **image:** make overlay lazy ([#35](https://github.com/Brooooooklyn/Image/issues/35)) ([3fd7d84](https://github.com/Brooooooklyn/Image/commit/3fd7d8434fba7e6d27461bec50bd65777f8b03fa))

## [1.4.4](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.4.3...@napi-rs/image@1.4.4) (2023-01-03)

**Note:** Version bump only for package @napi-rs/image

## [1.4.3](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.4.2...@napi-rs/image@1.4.3) (2023-01-03)

**Note:** Version bump only for package @napi-rs/image

## [1.4.2](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.4.1...@napi-rs/image@1.4.2) (2022-12-20)

### Bug Fixes

- **binding:** early return when input images are optimized ([#28](https://github.com/Brooooooklyn/Image/issues/28)) ([b695642](https://github.com/Brooooooklyn/Image/commit/b695642560e5aa43741e6a166119aa7b6d55145f))

## [1.4.1](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.4.0...@napi-rs/image@1.4.1) (2022-10-07)

**Note:** Version bump only for package @napi-rs/image

# [1.4.0](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.3.0...@napi-rs/image@1.4.0) (2022-08-23)

### Features

- **image:** upgrade libwebp to 0.7 ([#22](https://github.com/Brooooooklyn/Image/issues/22)) ([d3cde1c](https://github.com/Brooooooklyn/Image/commit/d3cde1c0e22bbd2b0e42ce604dcc668b6e364eb7))

# [1.3.0](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.2.0...@napi-rs/image@1.3.0) (2022-05-18)

### Features

- **image:** implement rawPixels and rawPixelsSync ([43e3938](https://github.com/Brooooooklyn/Image/commit/43e393860029cd3668aabf4d4362f8048faf4a6b))

# [1.2.0](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.1.2...@napi-rs/image@1.2.0) (2022-05-02)

### Features

- **image:** implement crop ([8bccc89](https://github.com/Brooooooklyn/Image/commit/8bccc89f54ede29897e156c01ce024ce9f13143b))
- **image:** support decode avif and webp ([#18](https://github.com/Brooooooklyn/Image/issues/18)) ([2813560](https://github.com/Brooooooklyn/Image/commit/2813560b9240c143d2c62fbea48d08918a9556af))

## [1.1.2](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.1.1...@napi-rs/image@1.1.2) (2022-04-22)

### Bug Fixes

- **image:** manipulate image has no effect ([e224c25](https://github.com/Brooooooklyn/Image/commit/e224c259d709bba704549ca34fa7851da41a6a3d))
- **image:** webp encode LumaA8 and Luma8 ([2473680](https://github.com/Brooooooklyn/Image/commit/24736809eaa38237bd618b5860b12ae0ebe91bd6))

## [1.1.1](https://github.com/Brooooooklyn/Image/compare/@napi-rs/image@1.1.0...@napi-rs/image@1.1.1) (2022-04-21)

### Bug Fixes

- **binding:** resize options and jpeg compress implementation ([b23c53b](https://github.com/Brooooooklyn/Image/commit/b23c53bf1085ef16b345a995fe130144dcf16a8f))

# 1.1.0 (2022-04-19)

### Features

- async Transformer class ([#9](https://github.com/Brooooooklyn/Image/issues/9)) ([7cd00d4](https://github.com/Brooooooklyn/Image/commit/7cd00d41814fb4a683c8b26762bbea558ebb87e2))
- **image:** implement png_quantize ([66f5e0f](https://github.com/Brooooooklyn/Image/commit/66f5e0f2ef1e8c692c87963f63994e030203cf14))
- **image:** implement svg_min ([5b916b3](https://github.com/Brooooooklyn/Image/commit/5b916b3c3cb93582eb0cbfccdf6a14e2d4deea65))
- **image:** support more operations on Transformer ([af8ed99](https://github.com/Brooooooklyn/Image/commit/af8ed994b74a3c8493bd5597b490ac636574c8a2))
- **image:** support Transformer from raw rgba pixels ([8d49a8c](https://github.com/Brooooooklyn/Image/commit/8d49a8c4d3e5e04f0c9ff66a07a1620d01241d67))
- support avif ([81fc73a](https://github.com/Brooooooklyn/Image/commit/81fc73a7ec3632160fbf17264ff7ec9306c08710))
- support webp ([e90ecdc](https://github.com/Brooooooklyn/Image/commit/e90ecdc4b97630a390982e5420790390891ade7c))
- transform into monorepo ([#3](https://github.com/Brooooooklyn/Image/issues/3)) ([d0de72e](https://github.com/Brooooooklyn/Image/commit/d0de72e2a884476878f49539c8bf4e85a7e1b2d1))
