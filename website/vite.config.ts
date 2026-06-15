import { defineConfig } from 'vite'
import { voidPlugin } from 'void'
import { voidReact } from '@void/react/plugin'
import { voidMarkdown } from '@void/md/plugin'
import tailwindcss from '@tailwindcss/vite'

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
        if (assetsGenerated) return
        assetsGenerated = true
        // Computed specifier so the config bundler does not statically resolve
        // (and require) this module at config-load time. The script is added in
        // a later migration group; until then `vite build` is intentionally not run.
        const mod = './scripts/build-assets.mjs'
        const { generateAssets } = (await import(/* @vite-ignore */ mod)) as {
          generateAssets: () => Promise<void>
        }
        await generateAssets()
      },
    },
  ],
})
