import SectionHeader from './SectionHeader'
import CodeBlock from './CodeBlock'
import Reveal from './_Reveal'
import { fullSample } from '../_data/samples'

export default function CodeSample({ html }: { html: string }) {
  return (
    <section className="border-t border-(--color-border)">
      <div className="container-page py-20 md:py-28">
        <SectionHeader
          index="05"
          label="PIPELINE"
          title={
            <>
              One <span className="text-(--color-accent)">pipeline</span>
            </>
          }
          subhead="Read image bytes, optimize PNG, or resize and convert to WebP and AVIF."
        />
        <Reveal className="mt-12">
          <CodeBlock html={html} copyText={fullSample} filename="optimize.ts" className="mx-auto max-w-3xl" />
        </Reveal>
      </div>
    </section>
  )
}
