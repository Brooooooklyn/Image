import { resolve } from 'node:path'
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

// Shared dev/preview middleware that (1) stamps CORP/COEP on every response so the
// cross-origin-isolated /playground can load its worker, wasm and image subresources,
// and (2) de-doubles the @napi-rs/image-wasm32-wasi nested-worker path that `vite dev`
// otherwise resolves with the package directory duplicated. See the plugin below.
const DUPLICATED_WASM_PKG = '@napi-rs/image-wasm32-wasi/@napi-rs/image-wasm32-wasi/'
function playgroundIsolationMiddleware(
  req: { url?: string },
  res: { setHeader: (name: string, value: string) => void },
  next: () => void,
) {
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
  ],
})
