export default function Home() {
  return (
    <section className="px-6 py-24 text-center">
      <h1 className="text-5xl font-bold tracking-tight">
        Fast image processing, <span className="text-(--color-accent)">in Rust</span>
      </h1>
      <p className="mt-4 text-(--color-muted)">
        Encode, compress, and transform images. Landing content lands in P2.
      </p>
      <p className="mt-8">
        <a className="text-(--color-accent) underline" href="/playground">
          Open the playground →
        </a>
      </p>
    </section>
  )
}
