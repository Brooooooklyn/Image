import { defineConfig } from 'vite'
import { voidPlugin } from 'void'
import { voidReact } from '@void/react/plugin'
import { voidMarkdown } from '@void/md/plugin'
import tailwindcss from '@tailwindcss/vite'

// Build-time asset generation (OG image, demo images, changelog) runs via the
// `build` npm script (`node scripts/build-assets.mjs && vite build`) instead of
// a buildStart hook. The hook approach is unreliable because Vite compiles this
// config into node_modules/.vite-temp, which breaks relative imports of the
// scripts dir; running the script first also guarantees the generated files
// (e.g. public/img/og.png) exist before Vite copies the public/ directory.
export default defineConfig({
  plugins: [
    voidPlugin(),
    voidReact(),
    voidMarkdown(), // enforce:'pre', auto-detects React → MUST come after voidReact()
    tailwindcss(),
  ],
})
