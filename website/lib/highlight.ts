import { createHighlighter, createJavaScriptRegexEngine, type Highlighter } from 'shiki'

// Shiki's default `codeToHtml` lazily instantiates the Oniguruma WASM regex engine.
// The Void SSR runtime is Cloudflare's workerd, which forbids runtime
// `WebAssembly.instantiate()` ("Wasm code generation disallowed by embedder") — so the
// WASM engine throws at request time and the page 500s, even though a Node-based
// `vite build` succeeds. Use Shiki's pure-JS regex engine instead (the documented
// Cloudflare-Workers-safe path). `forgiving: true` skips any grammar pattern the JS
// engine can't compile rather than throwing. We only ship one theme + one language, and
// the highlighter is created once and reused across requests.
const LANG = 'typescript'
const THEME = 'github-dark'

let highlighterPromise: Promise<Highlighter> | undefined
function getHighlighter() {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [THEME],
      langs: [LANG],
      engine: createJavaScriptRegexEngine({ forgiving: true }),
    })
  }
  return highlighterPromise
}

export async function highlight(code: string, lang = 'ts') {
  const highlighter = await getHighlighter()
  return highlighter.codeToHtml(code, {
    lang: lang === 'ts' ? LANG : lang,
    theme: THEME,
  })
}
