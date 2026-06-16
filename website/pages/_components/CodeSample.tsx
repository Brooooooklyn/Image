export default function CodeSample({ html }: { html: string }) {
  return (
    <section className="mx-auto max-w-4xl px-6 py-20">
      <h2 className="text-3xl font-bold">Three formats, one pipeline</h2>
      <div
        className="mt-8 overflow-x-auto rounded-lg border border-white/10 bg-black/30 p-4 text-sm [&_pre]:bg-transparent!"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </section>
  )
}
