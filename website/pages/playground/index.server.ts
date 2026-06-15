import { defineHead } from 'void'

// Island pages auto-prerender at deploy time. We opt OUT so the per-request
// COOP/COEP isolation headers from void.json (routing.headers /playground) are
// applied on every request — a prerendered static page would bypass them and
// the page would not be cross-origin isolated, breaking SharedArrayBuffer.
export const prerender = false

export const head = defineHead(() => ({ title: 'Playground' }))
