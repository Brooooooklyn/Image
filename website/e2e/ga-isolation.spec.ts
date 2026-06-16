import { test, expect } from '@playwright/test'

const GA_NEEDLES = ['googletagmanager', 'gtag(', 'dataLayer']

test('no GA in /playground SSR HTML (COEP-isolated page must stay GA-free)', async ({ request }) => {
  const res = await request.get('/playground')
  expect(res.status()).toBe(200)
  const html = await res.text()
  for (const needle of GA_NEEDLES) {
    expect(html, `/playground SSR HTML must not contain "${needle}"`).not.toContain(needle)
  }
})

test('no GA inlined into landing SSR HTML (GA is injected client-side)', async ({ request }) => {
  const res = await request.get('/')
  expect(res.status()).toBe(200)
  const html = await res.text()
  // GA must not be server-rendered into HTML anymore; it is added from a client useEffect.
  expect(html).not.toContain('googletagmanager')
})

test('GA loads client-side on the landing and fires page_view', async ({ page }) => {
  await page.goto('/')
  await expect
    .poll(() => page.evaluate(() => typeof (window as unknown as { gtag?: unknown }).gtag === 'function'), { timeout: 15_000 })
    .toBe(true)
  const pageViewFired = await page.evaluate(() => {
    const dl = (window as unknown as { dataLayer?: unknown[] }).dataLayer
    if (!Array.isArray(dl)) return false
    return dl.some((e) => { try { return Array.from(e as ArrayLike<unknown>).includes('page_view') } catch { return false } })
  })
  expect(pageViewFired).toBe(true)
  // And confirm the GA script tag was injected into <head>.
  expect(await page.evaluate(() => !!document.getElementById('ga-src'))).toBe(true)
})
