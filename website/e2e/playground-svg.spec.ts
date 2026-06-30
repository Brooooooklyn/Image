import { test, expect, type Page } from '@playwright/test'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

// SVG exercises the vector-only decode path: the generic `new Transformer(bytes)` constructor
// routes through `image::guess_format` (no SVG signature) and throws, so the worker must sniff
// SVG and rasterize via `Transformer.fromSvg`.
const dir = path.dirname(fileURLToPath(import.meta.url))
// The playground's own sample SVG (default `<svg xmlns=...>` root), served from public/.
const PLAIN_SVG = path.resolve(dir, '../public/img/input-debian.svg')
// A namespace-prefixed root (`<s:svg xmlns:s=...>`): valid SVG that `fromSvg` decodes but a naive
// `includes('<svg')` sniffer would miss. Regression guard for the byte sniffer.
const NS_PREFIXED_SVG = path.resolve(dir, 'fixtures/ns-prefixed.svg')

// Upload a file and wait for the worker to decode its metadata (status -> "idle", no error).
// `vite dev` pre-bundles the wasm worker's deps the first time the engine imports them, which
// forces a one-time full page reload (status resets to "empty"); retry the upload so that reload
// can't flake the run. Before the fix the worker called `new Transformer(svgBytes)`, which failed
// with "Guess format ... failed", so status never reached "idle".
async function uploadAndDecode(page: Page, fixture: string) {
  const status = page.getByTestId('pg-status')
  await expect(async () => {
    await page.locator('input[type=file]').setInputFiles(fixture)
    await expect(status).toHaveAttribute('data-status', 'idle', { timeout: 20_000 })
  }).toPass({ timeout: 90_000 })
  await expect(page.getByTestId('pg-error')).toHaveCount(0)
}

async function openPlayground(page: Page) {
  page.on('console', (msg) => console.log(`[browser:${msg.type()}] ${msg.text()}`))
  page.on('pageerror', (err) => console.log(`[pageerror] ${err.message}`))
  await page.goto('/playground')
  await expect
    .poll(() => page.evaluate(() => self.crossOriginIsolated), { timeout: 30_000 })
    .toBe(true)
}

test('playground decodes an SVG, reports its format, and converts it', async ({ page }) => {
  await openPlayground(page)
  await uploadAndDecode(page, PLAIN_SVG)

  // fromSvg reports format "svg": the metadata line shows it, and that string gates the Compress
  // tab (only jpeg/png inputs are compressible). If the binding mislabeled SVG as png/jpeg, the
  // Compress codecs would run on raw SVG bytes and fail — so assert the tab stays disabled.
  await expect(page.getByText(/·\s*SVG/).first()).toBeVisible()
  await page.getByRole('tab', { name: 'Compress' }).click()
  await expect(page.getByText(/supports JPEG and PNG inputs/i)).toBeVisible()
  await expect(page.getByRole('button', { name: 'Run' })).toBeDisabled()

  // Convert (the default-supported op) must produce real output bytes.
  await page.getByRole('tab', { name: 'Convert' }).click()
  const runButton = page.getByRole('button', { name: 'Run' })
  await expect(runButton).toBeEnabled({ timeout: 30_000 })
  await runButton.click()

  await expect(page.getByTestId('pg-status')).toHaveAttribute('data-status', 'done', {
    timeout: 120_000,
  })
  const text = await page.getByTestId('pg-bytes').innerText()
  expect(Number(text.match(/(\d+) bytes/)?.[1] ?? 0)).toBeGreaterThan(0)

  // The copyable snippet must reflect the SVG decode path the playground actually used, so a user
  // who copies it gets runnable code (`new Transformer(svg)` would throw "Guess format ... failed").
  await expect(page.getByTestId('pg-result')).toContainText('Transformer.fromSvg(input)')
})

test('playground decodes a namespace-prefixed SVG root', async ({ page }) => {
  await openPlayground(page)
  // `<s:svg xmlns:s=...>` must still be sniffed as SVG and routed through fromSvg, not rejected
  // by the generic constructor's format guess.
  await uploadAndDecode(page, NS_PREFIXED_SVG)
  await expect(page.getByText(/·\s*SVG/).first()).toBeVisible()
})
