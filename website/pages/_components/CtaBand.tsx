export default function CtaBand() {
  return (
    <section className="border-y border-(--color-accent)/20 bg-(--color-accent)/10 py-20 text-center">
      <div className="mx-auto max-w-2xl px-6">
        <h2 className="text-3xl font-bold tracking-tight">Try it in your browser</h2>
        <p className="mt-4 text-(--color-muted)">
          No install required. Compress, resize, and convert images right from your browser.
        </p>
        <div className="mt-8 flex items-center justify-center gap-4">
          <a
            href="/playground"
            className="rounded-lg bg-(--color-accent) px-5 py-2.5 font-medium text-black"
          >
            Open the playground
          </a>
          <a
            href="/docs"
            className="rounded-lg border border-(--color-accent)/40 px-5 py-2.5 font-medium text-(--color-accent) hover:border-(--color-accent)/70"
          >
            Read the docs
          </a>
        </div>
      </div>
    </section>
  )
}
