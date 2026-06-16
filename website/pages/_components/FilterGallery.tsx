import { filterDemos } from '../_data/showcase'

export default function FilterGallery() {
  return (
    <section className="mx-auto max-w-5xl px-6 py-20">
      <h2 className="text-center text-3xl font-bold tracking-tight">Built-in filters</h2>
      <p className="mx-auto mt-3 max-w-xl text-center text-(--color-muted)">
        Transform images with zero extra dependencies — all filters run natively in Rust.
      </p>
      <div className="mt-10 grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4">
        {filterDemos.map((d) => (
          <div key={d.label} className="flex flex-col gap-2">
            <img
              src={d.src}
              alt={d.label}
              loading="lazy"
              className="aspect-square w-full rounded-lg border border-white/10 object-cover"
            />
            <code className="text-center text-xs text-(--color-muted)">{d.label}</code>
          </div>
        ))}
      </div>
    </section>
  )
}
