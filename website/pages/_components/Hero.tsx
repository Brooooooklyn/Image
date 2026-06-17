import Button from './Button'
import CodeBlock from './CodeBlock'
import CountUp from './_CountUp'
import InstallSwitcher from './_InstallSwitcher'
import Reveal from './_Reveal'
import { benchThreadpool } from '../_data/benchmarks'
import { showcaseRows, pct } from '../_data/showcase'
import { formatRows } from '../_data/formats'
import { heroSample } from '../_data/samples'

const webp = benchThreadpool.find((b) => b.suite === 'WebP') ?? benchThreadpool[0]
const speed = Math.round((webp.napi / webp.sharp) * 10) / 10
const formats = formatRows.length
// Lossless only — the stat is labelled "lossless", so it must not borrow a lossy row's bigger %.
const savings = Math.max(...showcaseRows.filter((r) => r.kind === 'Lossless').map(pct))

const stats = [
  { node: <CountUp to={speed} decimals={1} suffix="×" />, label: 'faster WebP encode vs sharp' },
  { node: <CountUp to={formats} />, label: 'image formats' },
  { node: <CountUp to={savings} prefix="−" suffix="%" />, label: 'smaller, lossless' },
]

export default function Hero({ codeHtml }: { codeHtml: string }) {
  return (
    <section className="relative overflow-hidden">
      <div className="accent-glow" />
      <div className="container-page grid items-start gap-12 pt-20 pb-16 md:pt-28 lg:grid-cols-[minmax(0,1fr)_minmax(0,540px)] lg:gap-16">
        <div className="flex flex-col">
          <span className="eyebrow">NATIVE NODE ADDON · POWERED BY RUST</span>
          <h1 className="mt-5 font-display text-display-xl text-(--color-fg)">
            Fast image processing
            <br />
            <span className="text-(--color-accent)">in Rust.</span>
          </h1>
          <p className="mt-6 max-w-xl text-lg text-(--color-muted)">
            Encode, compress, resize and convert images — JPEG, PNG, WebP, AVIF and more — with a native Node addon that beats sharp.
          </p>
          <div className="mt-8 flex flex-wrap items-center gap-3">
            <Button variant="primary" href="/playground">Open the playground</Button>
            <Button variant="secondary" href="/docs">Read the docs</Button>
            <Button variant="ghost" href="https://github.com/Brooooooklyn/Image">GitHub</Button>
          </div>
          <div className="mt-8">
            <InstallSwitcher />
          </div>
        </div>

        <Reveal className="flex flex-col gap-6" delay={120}>
          <CodeBlock html={codeHtml} filename="transform.ts" copyText={heroSample} />
          <div className="grid grid-cols-3 gap-3">
            {stats.map((s) => (
              <div
                key={s.label}
                className="rounded-xl border border-(--color-border) bg-(--color-surface-1) px-4 py-5 transition-colors hover:border-(--color-border-strong)"
              >
                <div className="font-mono text-2xl tabular-nums text-(--color-fg) md:text-3xl">{s.node}</div>
                <div className="mt-2 font-mono text-[0.7rem] uppercase leading-snug tracking-wide text-(--color-faint)">
                  {s.label}
                </div>
              </div>
            ))}
          </div>
        </Reveal>
      </div>
    </section>
  )
}
