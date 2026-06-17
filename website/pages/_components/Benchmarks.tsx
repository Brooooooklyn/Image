import type { ReactNode } from 'react'
import { useEffect, useRef, useState } from 'react'
import { benchDefault, benchThreadpool, benchCaption, type Bench } from '../_data/benchmarks'
import SectionHeader from './SectionHeader'
import Chip from './Chip'
import Reveal from './_Reveal'
import CountUp from './_CountUp'

// Scale each suite to its own peak across both datasets. WebP (~431 ops/s) and
// AVIF (~36 ops/s) are different operations, so a single shared scale would crush
// the AVIF bars to slivers. Per-suite scaling keeps the napi-vs-sharp gap legible
// in both, while WebP's threadpool win still reads as a longer bar than default.
const suiteMax: Record<string, number> = {}
for (const r of [...benchDefault, ...benchThreadpool]) {
  suiteMax[r.suite] = Math.max(suiteMax[r.suite] ?? 0, r.napi, r.sharp)
}

// WebP threadpool is the headline win — compute it honestly in code.
const webpThreadpool = benchThreadpool.find((r) => r.suite === 'WebP')!
const webpRatio = webpThreadpool.napi / webpThreadpool.sharp

type BarTarget = { width: string; key: string }

// Client sub-component: bars render at full target width for SSR / no-JS,
// then grow from 0 once scrolled into view (unless reduced-motion).
function BarTrack({ targets, children }: { targets: BarTarget[]; children: (widths: Record<string, string>) => ReactNode }) {
  const finalWidths = Object.fromEntries(targets.map((t) => [t.key, t.width]))
  const [widths, setWidths] = useState<Record<string, string>>(finalWidths)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (typeof window === 'undefined' || typeof IntersectionObserver === 'undefined') return
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return

    const el = ref.current
    if (!el) return

    const zeroed = Object.fromEntries(targets.map((t) => [t.key, '0%']))
    setWidths(zeroed)

    const obs = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue
          obs.disconnect()
          // Next frame so the browser registers the 0% start before transitioning.
          requestAnimationFrame(() => setWidths(finalWidths))
        }
      },
      { threshold: 0.3 },
    )
    obs.observe(el)
    return () => obs.disconnect()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return <div ref={ref}>{children(widths)}</div>
}

function Bar({
  suite,
  napi,
  sharp,
  widths,
}: {
  suite: string
  napi: number
  sharp: number
  widths: Record<string, string>
}) {
  return (
    <div className="space-y-2.5">
      <div className="flex items-baseline justify-between">
        <span className="font-mono text-xs uppercase tracking-wider text-(--color-faint)">{suite}</span>
        <span className="font-mono text-xs tabular-nums text-(--color-accent)">
          {(napi / sharp).toFixed(2)}× sharp
        </span>
      </div>

      <div className="flex items-center gap-3">
        <span className="w-12 shrink-0 font-mono text-[10px] uppercase tracking-wider text-(--color-muted)">napi</span>
        <div className="h-7 flex-1 overflow-hidden rounded-md bg-(--color-bg)">
          <div
            className="h-full rounded-md bg-(--color-accent) transition-[width] duration-1000 ease-out"
            style={{ width: widths[`${suite}-napi`] }}
          />
        </div>
        <span className="w-20 shrink-0 text-right font-mono text-xs tabular-nums text-(--color-accent)">{napi} ops/s</span>
      </div>

      <div className="flex items-center gap-3">
        <span className="w-12 shrink-0 font-mono text-[10px] uppercase tracking-wider text-(--color-muted)">sharp</span>
        <div className="h-7 flex-1 overflow-hidden rounded-md bg-(--color-bg)">
          <div
            className="h-full rounded-md bg-(--color-faint) transition-[width] duration-1000 ease-out"
            style={{ width: widths[`${suite}-sharp`] }}
          />
        </div>
        <span className="w-20 shrink-0 text-right font-mono text-xs tabular-nums text-(--color-muted)">{sharp} ops/s</span>
      </div>
    </div>
  )
}

function BenchColumn({ label, accent, rows }: { label: string; accent?: boolean; rows: Bench[] }) {
  const targets: BarTarget[] = rows.flatMap((r) => [
    { key: `${r.suite}-napi`, width: `${(r.napi / suiteMax[r.suite]) * 100}%` },
    { key: `${r.suite}-sharp`, width: `${(r.sharp / suiteMax[r.suite]) * 100}%` },
  ])

  return (
    <div className="rounded-xl border border-(--color-border) bg-(--color-surface-1) p-6 transition-colors hover:border-(--color-border-strong) md:p-7">
      <div className="mb-6 flex items-center justify-between">
        <span className="font-mono text-xs uppercase tracking-wider text-(--color-muted)">{label}</span>
        {accent ? <Chip tone="accent">fastest</Chip> : <Chip tone="muted">baseline</Chip>}
      </div>
      <BarTrack targets={targets}>
        {(widths) => (
          <div className="space-y-7">
            {rows.map((r) => (
              <Bar key={r.suite} suite={r.suite} napi={r.napi} sharp={r.sharp} widths={widths} />
            ))}
          </div>
        )}
      </BarTrack>
    </div>
  )
}

export default function Benchmarks() {
  return (
    <section className="border-t border-(--color-border)">
      <div className="container-page py-20 md:py-28">
        <SectionHeader
          index="01"
          label="BENCHMARK"
          title={
            <>
              Faster than <span className="text-(--color-accent)">sharp</span>
            </>
          }
          subhead={benchCaption}
        />

        <Reveal className="mt-12">
          <div className="grid gap-10 lg:grid-cols-[minmax(0,18rem)_minmax(0,1fr)] lg:items-center lg:gap-12">
            <div className="relative">
              <div className="accent-glow" aria-hidden />
              <p className="eyebrow">peak speedup</p>
              <p className="mt-3 font-display text-display-lg leading-none text-(--color-fg)">
                <CountUp to={webpRatio} decimals={1} suffix="×" />
              </p>
              <p className="mt-4 max-w-xs text-sm text-(--color-muted)">
                faster WebP encode with{' '}
                <span className="font-mono text-(--color-fg)">UV_THREADPOOL_SIZE=10</span>. AVIF lands at
                near-parity — WebP is where the gap opens up.
              </p>
            </div>

            <div className="grid gap-6 md:grid-cols-2">
              <BenchColumn label="default" rows={benchDefault} />
              <BenchColumn label="UV_THREADPOOL_SIZE=10" accent rows={benchThreadpool} />
            </div>
          </div>

          <p className="mt-8 font-mono text-xs leading-relaxed text-(--color-faint)">{benchCaption}</p>
        </Reveal>
      </div>
    </section>
  )
}
