import { defineHandler, defineHead, type InferProps } from 'void'
import { highlight } from '../lib/highlight'
import { heroSample, fullSample } from './_data/samples'

// index.html must never be served from cache — keep it pure SSR (no prerender, plus
// revalidate:0 in void.json) so a deploy is live on the very next request, with no
// edge/ISR copy that could go stale. The shiki highlighter is a module-level
// singleton, so per-request rendering stays cheap.
export const prerender = false

export const loader = defineHandler(async () => ({
  heroHtml: await highlight(heroSample),
  fullHtml: await highlight(fullSample),
}))

export type Props = InferProps<typeof loader>

const SITE_URL = 'https://image.napi.rs'
const DESCRIPTION =
  'Encode, compress, resize and convert JPEG/PNG/WebP/AVIF with a native Node addon faster than sharp.'

export const head = defineHead<Props>(() => ({
  title: 'Fast image processing in Rust',
  meta: [
    { name: 'description', content: DESCRIPTION },
    { property: 'og:type', content: 'website' },
    { property: 'og:site_name', content: '@napi-rs/image' },
    { property: 'og:url', content: `${SITE_URL}/` },
    { property: 'og:title', content: '@napi-rs/image' },
    { property: 'og:description', content: DESCRIPTION },
    { property: 'og:image', content: `${SITE_URL}/img/og.png` },
    { property: 'og:image:width', content: '1200' },
    { property: 'og:image:height', content: '630' },
    { property: 'og:image:alt', content: 'Image processing for Node.js and the browser' },
    { name: 'twitter:card', content: 'summary_large_image' },
  ],
}))
