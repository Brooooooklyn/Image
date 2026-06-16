import BeforeAfter from './_BeforeAfter'
import { showcaseRows, pct, kb } from '../_data/showcase'

export default function OptimizationShowcase() {
  const [featured, ...rest] = showcaseRows

  return (
    <section className="mx-auto max-w-5xl px-6 py-20">
      <h2 className="text-center text-3xl font-bold tracking-tight">See the bytes disappear</h2>
      <p className="mx-auto mt-3 max-w-xl text-center text-(--color-muted)">
        Drag the slider to compare original and optimized — same image, a fraction of the size.
      </p>

      {/* Featured row */}
      <div className="mt-10 rounded-xl border border-white/10 bg-white/[0.03] p-6">
        <BeforeAfter
          before={featured.before}
          after={featured.after}
          beforeLabel="original"
          afterLabel={featured.label}
        />
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
          <code className="text-sm text-(--color-muted)">{featured.label}</code>
          <div className="flex items-center gap-4 text-sm">
            <span className="text-(--color-muted)">{kb(featured.beforeBytes)} → {kb(featured.afterBytes)}</span>
            <span className="font-bold text-(--color-accent)">−{pct(featured)}%</span>
          </div>
        </div>
      </div>

      {/* Remaining rows grid */}
      <div className="mt-6 grid gap-4 sm:grid-cols-2">
        {rest.map((row) => (
          <div key={row.label} className="rounded-xl border border-white/10 bg-white/[0.03] p-4">
            <BeforeAfter
              before={row.before}
              after={row.after}
              beforeLabel="original"
              afterLabel={row.label}
            />
            <div className="mt-3 flex flex-wrap items-start justify-between gap-2">
              <div className="min-w-0">
                <code className="block truncate text-xs text-(--color-muted)">{row.label}</code>
                <span className={`mt-1 inline-block rounded px-1.5 py-0.5 text-xs ${row.kind === 'Lossless' ? 'bg-green-900/40 text-green-400' : 'bg-yellow-900/40 text-yellow-400'}`}>
                  {row.kind}
                </span>
              </div>
              <div className="flex flex-col items-end text-xs">
                <span className="text-(--color-muted)">{kb(row.beforeBytes)} → {kb(row.afterBytes)}</span>
                <span className="font-semibold text-(--color-accent)">−{pct(row)}%</span>
              </div>
            </div>
          </div>
        ))}
      </div>
    </section>
  )
}
