import { resolve } from 'node:path'
import { readdirSync, rmSync } from 'node:fs'
import { pathToFileURL } from 'node:url'
import { defineConfig } from 'vite'
import { voidPlugin } from 'void'
import { voidReact } from '@void/react/plugin'
import { voidMarkdown } from '@void/md/plugin'
import tailwindcss from '@tailwindcss/vite'

// Build-time asset generation (OG image, demo images, changelog) runs via a
// `buildStart` hook so it executes for BOTH local `vite build` AND `void deploy`
// (which invokes the `vite build` binary directly, bypassing the package.json
// `build` script). The script is resolved by ABSOLUTE path because Vite compiles
// this config into node_modules/.vite-temp, which breaks relative imports of the
// scripts dir. `vite build` always runs with cwd = website/, so resolving from
// process.cwd() is stable. buildStart fires early enough that the generated files
// (e.g. public/img/og.png) exist before Vite copies the public/ directory.
let assetsGenerated = false

// Shared dev/preview middleware that (1) stamps COOP/COEP on the document and CORP on
// every response so the cross-origin-isolated /playground can load its worker, wasm and
// image subresources, and (2) de-doubles the @napi-rs/image-wasm32-wasi nested-worker
// path that `vite dev` otherwise resolves with the package directory duplicated.
//
// COOP is the easy one to forget: `self.crossOriginIsolated` (which gates
// SharedArrayBuffer, and therefore the wasm threads the playground needs) is only true
// when the DOCUMENT carries BOTH Cross-Origin-Opener-Policy: same-origin AND
// Cross-Origin-Embedder-Policy: require-corp. The deployed worker sets both for
// /playground via void.json `routing.headers`; `vite preview` does not replay those, so
// without COOP here the preview document is not isolated and the island falls back to its
// StaticFallback ("In-browser demo unavailable") instead of the interactive UI. See the
// plugin below.
const DUPLICATED_WASM_PKG = '@napi-rs/image-wasm32-wasi/@napi-rs/image-wasm32-wasi/'
function playgroundIsolationMiddleware(
  req: { url?: string },
  res: { setHeader: (name: string, value: string) => void },
  next: () => void,
) {
  res.setHeader('Cross-Origin-Opener-Policy', 'same-origin')
  res.setHeader('Cross-Origin-Resource-Policy', 'same-origin')
  res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp')
  if (req.url && req.url.includes(DUPLICATED_WASM_PKG)) {
    req.url = req.url.replace(DUPLICATED_WASM_PKG, '@napi-rs/image-wasm32-wasi/')
  }
  next()
}

export default defineConfig({
  // The @napi-rs/image browser build is a top-level-await ESM that spawns its
  // own internal module worker (wasi-worker-browser.mjs). Our playground runs it
  // inside our own Worker, so BOTH worker layers must be emitted as ES modules —
  // the default 'iife' worker format cannot handle the top-level await in the
  // wasm package, which fails the build. format:'es' fixes the nested worker.
  worker: { format: 'es' },
  resolve: {
    // The playground worker imports the public `@napi-rs/image` API, but in the
    // browser that wrapper's `main` (index.js) is the universal NATIVE loader — it
    // `require()`s a platform .node binary (e.g. @napi-rs/image-darwin-arm64), and
    // Vite/rolldown dies trying to parse that binary as JS ("stream did not contain
    // valid UTF-8"). The package's `browser` field already points at browser.js,
    // which is just `export * from '@napi-rs/image-wasm32-wasi'`, so alias straight
    // to the wasm build everywhere Vite bundles. The native loader is never touched.
    // (This only surfaced after a clean install of the published release pulled the
    // native package in; the build-time asset script runs as a plain Node import and
    // is unaffected by this alias.)
    alias: {
      '@napi-rs/image': '@napi-rs/image-wasm32-wasi',
    },
  },
  plugins: [
    // Make the cross-origin-isolated /playground actually run the @napi-rs/image wasm
    // worker under `vite dev` / `vite preview`. Two dev-only gaps to bridge:
    //
    // 1) CORP on subresources. The /playground document is served cross-origin-isolated
    //    (COOP:same-origin + COEP:require-corp, set per-route in void.json) so the wasm can
    //    use SharedArrayBuffer + threads. Under COEP:require-corp the browser blocks EVERY
    //    subresource the isolated page loads — the playground worker module, the nested
    //    wasi-worker-browser.mjs, the .wasm binary, the sample image — unless each one
    //    carries a Cross-Origin-Resource-Policy header. The Vite dev/preview server does
    //    not stamp CORP on the modules/assets it serves, so without this the worker fails
    //    with ERR_BLOCKED_BY_RESPONSE and the page hangs. We add CORP (+ COEP) here.
    //
    // 2) Nested worker path doubling. @napi-rs/image-wasm32-wasi spawns its own module
    //    worker via `new Worker(new URL('@napi-rs/image-wasm32-wasi/wasi-worker-browser.mjs',
    //    import.meta.url))`. Under `vite dev` that bare-specifier-in-new-URL resolves with
    //    the package directory doubled (…/@napi-rs/image-wasm32-wasi/@napi-rs/
    //    image-wasm32-wasi/wasi-worker-browser.mjs), which 404s. We collapse the duplicated
    //    segment back to the real file. A production `vite build` bundles this worker, so
    //    both fixes are dev/preview only (apply:'serve'); the deployed worker must still
    //    apply equivalent CORP/COEP headers to these assets at the edge.
    {
      name: 'playground-isolation-dev',
      apply: 'serve',
      configureServer(server) {
        server.middlewares.use(playgroundIsolationMiddleware)
      },
      configurePreviewServer(server) {
        server.middlewares.use(playgroundIsolationMiddleware)
      },
    },
    voidPlugin(),
    voidReact(),
    voidMarkdown(), // enforce:'pre', auto-detects React → MUST come after voidReact()
    tailwindcss(),
    {
      name: 'gen-build-assets',
      apply: 'build',
      async buildStart() {
        if (assetsGenerated) return // buildStart fires per-environment (client+worker); run once
        assetsGenerated = true
        const mod = pathToFileURL(resolve(process.cwd(), 'scripts/build-assets.mjs')).href
        const { generateAssets } = await import(/* @vite-ignore */ mod)
        await generateAssets()
      },
    },
    // Prune benchmark leftovers from the client bundle.
    //
    // public/img/ contains benchmark INPUT files (nasa-4928x3279.png is a 13 MB
    // symlink to the repo root; sharp.mjs is the benchmark script that reads it).
    // They are NOT web assets and must NOT ship to production.
    //
    // Why post-build deletion instead of publicDir exclusion:
    //   Vite copies the entire publicDir wholesale — there is no per-file ignore
    //   list. Adding an explicit exclusion API does not exist in Vite's publicDir
    //   config. We therefore let Vite copy everything and then delete the unwanted
    //   files from dist/client AFTER the client bundle is written.
    //
    // Guard: closeBundle fires once per build environment (client + SSR/worker).
    // We only act on the client output path. rmSync with { force: true } is a
    // no-op when the file was already gone (e.g. second env invocation, or the
    // file was never present in an earlier build).
    {
      name: 'prune-benchmark-leftovers',
      apply: 'build',
      closeBundle() {
        const imgDir = resolve(process.cwd(), 'dist/client/img')

        // Known benchmark inputs — NOT web assets.
        const knownJunk = [
          resolve(imgDir, 'nasa-4928x3279.png'),
          resolve(imgDir, 'sharp.mjs'),
        ]
        for (const p of knownJunk) {
          rmSync(p, { force: true })
        }

        // Also sweep for any stray *.mjs files that may have crept in.
        let entries: string[] = []
        try { entries = readdirSync(imgDir) } catch { /* dir may not exist on SSR pass */ }
        for (const name of entries) {
          if (name.endsWith('.mjs')) {
            rmSync(resolve(imgDir, name), { force: true })
          }
        }
      },
    },
  ],
})
