import { defineConfig } from '@playwright/test'

const BASE_URL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173'

export default defineConfig({
  testDir: './e2e',
  timeout: 120_000,
  use: { baseURL: BASE_URL },
  // Auto-manage the dev server so the test is self-contained. Locally we reuse an
  // already-running dev server for fast iteration, but in CI we always boot a fresh
  // one so the gate can never pass against a stale server that lacks the vite.config
  // isolation middleware this test depends on.
  webServer: {
    command: 'npm run dev',
    url: `${BASE_URL}/`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
})
