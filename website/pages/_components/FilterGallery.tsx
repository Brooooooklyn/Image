import { filterDemos } from '../_data/showcase'
import SectionHeader from './SectionHeader'
import Reveal from './_Reveal'
import Chip from './Chip'

export default function FilterGallery() {
  return (
    <section className="border-t border-(--color-border)">
      <div className="container-page py-20 md:py-28">
        <SectionHeader
          index="04"
          label="FILTERS"
          title={<>Built-in <span className="text-(--color-accent)">filters</span></>}
          subhead="grayscale, blur, hue-rotate, contrast and more — applied natively in Rust."
        />
        <Reveal className="mt-12">
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-4">
            {filterDemos.map((d, i) => (
              <Reveal key={d.label} delay={i * 60}>
                <div className="group rounded-xl border border-(--color-border) bg-(--color-surface-1) overflow-hidden transition hover:border-(--color-border-strong) hover:-translate-y-0.5">
                  <div className="aspect-[3/2] w-full overflow-hidden">
                    <img
                      src={d.src}
                      alt={d.label}
                      loading="lazy"
                      className="w-full h-full object-cover transition duration-300 group-hover:scale-[1.02]"
                    />
                  </div>
                  <div className="px-3 py-2.5">
                    <Chip tone="muted" className="tabular-nums">
                      {d.label}
                    </Chip>
                  </div>
                </div>
              </Reveal>
            ))}
          </div>
        </Reveal>
      </div>
    </section>
  )
}
