import type { Props } from './index.server'
import Hero from './_components/Hero'
import Benchmarks from './_components/Benchmarks'
import OptimizationShowcase from './_components/OptimizationShowcase'
import FormatMatrix from './_components/FormatMatrix'
import FilterGallery from './_components/FilterGallery'
import CodeSample from './_components/CodeSample'
import CtaBand from './_components/CtaBand'

export default function Home({ heroHtml, fullHtml }: Props) {
  return (
    <>
      <Hero codeHtml={heroHtml} />
      <Benchmarks />
      <OptimizationShowcase />
      <FormatMatrix />
      <FilterGallery />
      <CodeSample html={fullHtml} />
      <CtaBand />
    </>
  )
}
