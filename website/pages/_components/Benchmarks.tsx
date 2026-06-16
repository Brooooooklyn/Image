import { benchDefault, benchThreadpool, benchCaption, type Bench } from '../_data/benchmarks'

function Bars({ rows }: { rows: Bench[] }) {
  const max = Math.max(...rows.flatMap((r) => [r.napi, r.sharp]))
  return (
    <div className="mt-4 space-y-6">
      {rows.map((r) => (
        <div key={r.suite}>
          <p className="mb-2 text-sm font-semibold">{r.suite}</p>
          <div className="space-y-1">
            <div className="flex items-center gap-3">
              <span className="w-16 text-right text-xs text-(--color-muted)">napi-rs</span>
              <div
                className="h-5 rounded-sm bg-(--color-accent)"
                style={{ width: `${(r.napi / max) * 100}%` }}
              />
              <span className="text-xs text-(--color-accent)">{r.napi} ops/s</span>
            </div>
            <div className="flex items-center gap-3">
              <span className="w-16 text-right text-xs text-(--color-muted)">sharp</span>
              <div
                className="h-5 rounded-sm bg-white/20"
                style={{ width: `${(r.sharp / max) * 100}%` }}
              />
              <span className="text-xs text-(--color-muted)">{r.sharp} ops/s</span>
            </div>
          </div>
        </div>
      ))}
    </div>
  )
}

export default function Benchmarks() {
  return (
    <section className="mx-auto max-w-4xl px-6 py-20">
      <h2 className="text-3xl font-bold">Faster than sharp</h2>
      <p className="mt-2 text-(--color-muted)">{benchCaption}</p>

      <div className="mt-10 grid gap-12 md:grid-cols-2">
        <div>
          <p className="text-sm font-semibold uppercase tracking-widest text-(--color-muted)">default</p>
          <Bars rows={benchDefault} />
        </div>
        <div>
          <p className="text-sm font-semibold uppercase tracking-widest text-(--color-muted)">UV_THREADPOOL_SIZE=10</p>
          <Bars rows={benchThreadpool} />
        </div>
      </div>
    </section>
  )
}
