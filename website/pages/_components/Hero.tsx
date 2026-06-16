import InstallCommand from './InstallCommand'
import HeroCodeSample from './HeroCodeSample'

const badges = [
  { src: 'https://img.shields.io/npm/v/@napi-rs/image.svg', href: 'https://www.npmjs.com/package/@napi-rs/image', alt: 'npm version' },
  { src: 'https://packagephobia.com/badge?p=@napi-rs/image', href: 'https://packagephobia.com/result?p=@napi-rs/image', alt: 'install size' },
  { src: 'https://img.shields.io/npm/dm/@napi-rs/image.svg', href: 'https://npmcharts.com/compare/@napi-rs/image?minimal=true', alt: 'downloads' },
]

export default function Hero({ codeHtml }: { codeHtml: string }) {
  return (
    <section className="relative mx-auto max-w-5xl px-6 pt-24 pb-16 text-center">
      {/* Subtle radial accent glow behind the headline */}
      <div
        className="pointer-events-none absolute inset-x-0 top-0 -z-10 h-64"
        style={{ background: 'radial-gradient(ellipse 60% 50% at 50% 0%, oklch(72% 0.23 250 / 0.15), transparent)' }}
      />
      <h1 className="text-5xl font-bold tracking-tight md:text-6xl">
        Fast image processing, <span className="text-(--color-accent)">in Rust</span>
      </h1>
      <p className="mx-auto mt-5 max-w-2xl text-lg text-(--color-muted)">
        Encode, compress, resize and convert images — JPEG, PNG, WebP, AVIF and more — with a native Node addon that beats sharp.
      </p>
      <div className="mt-8 flex items-center justify-center gap-4">
        <a href="/playground" className="rounded-lg bg-(--color-accent) px-5 py-2.5 font-medium text-black">Try the playground</a>
        <a href="/docs" className="rounded-lg border border-white/15 px-5 py-2.5 font-medium">Read the docs</a>
      </div>
      <div className="mt-6 flex items-center justify-center gap-3">
        {badges.map((b) => (
          <a key={b.alt} href={b.href}>
            <img src={b.src} alt={b.alt} loading="lazy" />
          </a>
        ))}
      </div>
      <InstallCommand />
      <div className="mx-auto mt-12 max-w-2xl text-left">
        <HeroCodeSample html={codeHtml} />
      </div>
    </section>
  )
}
