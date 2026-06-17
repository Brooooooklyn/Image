import SectionHeader from './SectionHeader'
import Chip from './Chip'
import Reveal from './_Reveal'
import CountUp from './_CountUp'
import BeforeAfter from './_BeforeAfter'
import { showcaseRows, pct, kb } from '../_data/showcase'

export default function OptimizationShowcase() {
  const [featured, ...rest] = showcaseRows

  return (
    <section className="border-t border-(--color-border)">
      <div className="container-page py-20 md:py-28">
        <SectionHeader
          index="02"
          label="COMPRESSION"
          title={
            <>
              See the <span className="text-(--color-accent)">bytes</span> disappear
            </>
          }
          subhead="Drag to compare original and optimized — same image, a fraction of the size."
        />

        {/* Featured row */}
        <Reveal className="mt-12">
          <div className="rounded-xl border border-(--color-border) bg-(--color-surface-1) p-6 transition-colors hover:border-(--color-border-strong) md:p-8">
            <BeforeAfter
              before={featured.before}
              after={featured.after}
              beforeLabel="original"
              afterLabel={featured.label}
            />
            <div className="mt-6 flex flex-wrap items-center justify-between gap-4">
              <div className="flex min-w-0 flex-wrap items-center gap-3">
                <code className="font-mono text-sm text-(--color-fg)">{featured.label}</code>
                <Chip tone={featured.kind === 'Lossless' ? 'accent' : 'muted'}>{featured.kind}</Chip>
              </div>
              <div className="flex items-center gap-4 font-mono text-sm tabular-nums">
                <span className="text-(--color-muted)">
                  {kb(featured.beforeBytes)} → {kb(featured.afterBytes)}
                </span>
                <CountUp
                  to={pct(featured)}
                  prefix="−"
                  suffix="%"
                  className="text-base font-medium text-(--color-accent)"
                />
              </div>
            </div>
          </div>
        </Reveal>

        {/* Remaining rows grid */}
        <Reveal className="mt-6">
          <div className="grid gap-6 sm:grid-cols-2">
            {rest.map((row) => (
              <div
                key={row.label}
                className="rounded-xl border border-(--color-border) bg-(--color-surface-1) p-5 transition-colors hover:border-(--color-border-strong)"
              >
                <BeforeAfter
                  before={row.before}
                  after={row.after}
                  beforeLabel="original"
                  afterLabel={row.label}
                />
                <div className="mt-4 flex flex-wrap items-start justify-between gap-3">
                  <div className="flex min-w-0 flex-col items-start gap-2">
                    <code className="block max-w-full truncate font-mono text-xs text-(--color-fg)">
                      {row.label}
                    </code>
                    <Chip tone={row.kind === 'Lossless' ? 'accent' : 'muted'}>{row.kind}</Chip>
                  </div>
                  <div className="flex flex-col items-end gap-1 font-mono text-xs tabular-nums">
                    <span className="text-(--color-muted)">
                      {kb(row.beforeBytes)} → {kb(row.afterBytes)}
                    </span>
                    <span className="font-medium text-(--color-accent)">−{pct(row)}%</span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </Reveal>
      </div>
    </section>
  )
}
