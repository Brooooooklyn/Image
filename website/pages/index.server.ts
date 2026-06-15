import { defineHead } from 'void'

export const prerender = true

export const head = defineHead(() => ({
  title: 'Fast image processing in Rust',
  meta: [{ name: 'description', content: 'Encode, compress, and transform images with @napi-rs/image.' }],
}))
