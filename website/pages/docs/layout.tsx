import { type ReactNode } from 'react'

// Explicit, ordered docs navigation. Kept as a hand-maintained list (rather than
// auto-discovered from @void/md/pages) so the order and labels are deterministic —
// docs read top-to-bottom, and that order is editorial, not alphabetical.
//
// No active-link highlight: these are fully server-rendered markdown pages that
// don't hydrate, and a nested layout's router only exposes its mounted base
// ('/docs'), not the leaf route — so the current page can't be resolved here
// without client JS the page never ships. The sidebar is a plain nav list.
const NAV = [
  { href: '/docs', label: 'Getting Started' },
  { href: '/docs/api', label: 'API Reference' },
  { href: '/docs/formats', label: 'Format Guides' },
  { href: '/docs/recipes', label: 'Recipes' },
  { href: '/docs/credits', label: 'Credits' },
]

export default function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="mx-auto flex max-w-6xl gap-10 px-6 py-12 lg:gap-16">
      <aside className="hidden w-48 shrink-0 md:block">
        <nav className="sticky top-12 flex flex-col gap-1 text-sm">
          <p className="mb-2 font-mono text-xs uppercase tracking-wider text-(--color-muted) opacity-60">
            Documentation
          </p>
          {NAV.map((item) => (
            <a
              key={item.href}
              href={item.href}
              className="border-l-2 border-transparent pl-3 text-(--color-muted) transition-colors hover:border-(--color-accent) hover:text-(--color-fg)"
            >
              {item.label}
            </a>
          ))}
        </nav>
      </aside>
      <article className="void-md min-w-0 flex-1">{children}</article>
    </div>
  )
}
