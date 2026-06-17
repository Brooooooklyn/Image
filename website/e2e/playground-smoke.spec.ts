import { test, expect } from '@playwright/test'

test('playground is cross-origin isolated and the worker encodes the sample', async ({ page }) => {
  // Surface browser-side failures (worker load errors, wasm faults, 404s) in the
  // Playwright output so a non-`done` status is debuggable rather than opaque.
  page.on('console', (msg) => console.log(`[browser:${msg.type()}] ${msg.text()}`))
  page.on('pageerror', (err) => console.log(`[pageerror] ${err.message}`))

  await page.goto('/playground')

  await expect
    .poll(() => page.evaluate(() => self.crossOriginIsolated), { timeout: 30_000 })
    .toBe(true)

  // Drive the real flow — nothing auto-runs. Load the bundled sample, wait for the
  // worker to decode its metadata (which enables Run), then run the default Convert op.
  await page.getByRole('button', { name: 'Use sample image' }).click()
  const runButton = page.getByRole('button', { name: 'Run' })
  await expect(runButton).toBeEnabled({ timeout: 30_000 })
  await runButton.click()

  await expect(page.getByTestId('pg-status')).toHaveAttribute('data-status', 'done', {
    timeout: 120_000,
  })

  const text = await page.getByTestId('pg-bytes').innerText()
  expect(Number(text.match(/(\d+) bytes/)?.[1] ?? 0)).toBeGreaterThan(0)
})
