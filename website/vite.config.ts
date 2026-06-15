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

export default defineConfig({
  plugins: [
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
