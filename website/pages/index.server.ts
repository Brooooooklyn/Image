import { defineHandler, defineHead, type InferProps } from 'void'
import { highlight } from '../lib/highlight'
import { heroSample, fullSample } from './_data/samples'

export const prerender = true

export const loader = defineHandler(async () => ({
  heroHtml: await highlight(heroSample),
  fullHtml: await highlight(fullSample),
}))

export type Props = InferProps<typeof loader>

export const head = defineHead<Props>(() => ({
  title: 'Fast image processing in Rust',
  meta: [
    { name: 'description', content: 'Encode, compress, resize and convert JPEG/PNG/WebP/AVIF with a native Node addon faster than sharp.' },
    { property: 'og:image', content: '/img/og.png' },
    { property: 'og:title', content: '@napi-rs/image' },
    { name: 'twitter:card', content: 'summary_large_image' },
  ],
}))
