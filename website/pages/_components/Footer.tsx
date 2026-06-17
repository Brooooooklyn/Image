const links = [
  { label: 'Playground', href: '/playground' },
  { label: 'Docs', href: '/docs' },
  { label: 'Changelog', href: '/changelog' },
  { label: 'GitHub', href: 'https://github.com/Brooooooklyn/Image' },
  { label: 'npm', href: 'https://npmx.dev/@napi-rs/image' },
  { label: 'Discord', href: 'https://discord.gg/SpWzYHsKHs' },
]

export default function Footer() {
  return (
    <footer className="border-t border-(--color-border)">
      <div className="container-page py-12">
        <div className="flex flex-col gap-8 md:flex-row md:items-start md:justify-between">
          <div className="flex flex-col gap-3">
            <span className="font-mono text-sm font-medium text-(--color-fg) tracking-tight">
              @napi-rs/image
            </span>
            <p className="text-sm text-(--color-muted) max-w-xs">
              High-performance image processing for Node.js — native speed, WebAssembly portable.
            </p>
          </div>

          <nav className="flex flex-wrap gap-x-8 gap-y-2">
            {links.map((l) => (
              <a
                key={l.label}
                href={l.href}
                className="text-sm text-(--color-muted) hover:text-(--color-fg) transition-colors"
              >
                {l.label}
              </a>
            ))}
          </nav>
        </div>

        <div className="mt-10 pt-6 border-t border-(--color-border)">
          <p className="font-mono text-xs text-(--color-faint) tabular-nums">
            Built with @napi-rs · MIT licensed
          </p>
        </div>
      </div>
    </footer>
  )
}
