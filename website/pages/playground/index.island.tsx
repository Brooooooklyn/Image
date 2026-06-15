import Playground from './_Playground' with { island: 'load' }
export default function PlaygroundPage() {
  return (
    <section>
      <h1 className="px-6 pt-12 text-3xl font-bold">Playground (smoke test)</h1>
      <Playground />
    </section>
  )
}
