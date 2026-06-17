import { type ReactNode } from 'react'

// The changelog is a single long markdown page (regenerated from GitHub releases
// at build time). Wrap it in `.void-md` so the @void/md prose theme applies, and
// cap it to a readable centered column — the root layout only provides the header
// and footer chrome, not a content container.
export default function ChangelogLayout({ children }: { children: ReactNode }) {
  return <div className="void-md mx-auto max-w-3xl px-6 py-12">{children}</div>
}
