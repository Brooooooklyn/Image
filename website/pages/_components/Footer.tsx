// All badges must be served with `Cross-Origin-Resource-Policy: cross-origin` so they load on the
// COEP:require-corp /playground document. shields.io sends that header; packagephobia does not (and
// rate-limits to 429), so its install-size badge was dropped — there is no CORP-safe install-size
// source for a native addon (bundlephobia only measures the JS, missing the platform .node binaries).
const badges = [
  { src: 'https://img.shields.io/npm/v/@napi-rs/image.svg', href: 'https://www.npmjs.com/package/@napi-rs/image', alt: 'npm version' },
  { src: 'https://img.shields.io/npm/dm/@napi-rs/image.svg', href: 'https://npmcharts.com/compare/@napi-rs/image?minimal=true', alt: 'downloads' },
]

export default function Footer() {
  return (
    <footer className="border-t border-white/10 px-6 py-10 text-(--color-muted) text-sm">
      <div className="mx-auto max-w-5xl flex flex-col gap-6 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-4">
            <a href="https://github.com/Brooooooklyn/Image" className="hover:text-(--color-fg)">GitHub</a>
            <a href="https://www.npmjs.com/package/@napi-rs/image" className="hover:text-(--color-fg)">npm</a>
            <a href="https://discord.gg/SpWzYHsKHs" className="hover:text-(--color-fg)">Discord</a>
          </div>
          <p className="text-xs opacity-60">Built with @napi-rs · MIT licensed</p>
        </div>
        <div className="flex items-center gap-3">
          {badges.map((b) => (
            <a key={b.alt} href={b.href}>
              <img src={b.src} alt={b.alt} loading="lazy" />
            </a>
          ))}
        </div>
      </div>
    </footer>
  )
}
