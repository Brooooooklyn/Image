import { writeFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'

const packageName = '@napi-rs/image'
const locale = 'en'

export async function generateChangelog() {
  const headers = {
    Accept: 'application/vnd.github+json',
  }
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `token ${process.env.GITHUB_TOKEN}`
  }

  const releases = await fetch(`https://api.github.com/repos/Brooooooklyn/Image/releases?per_page=100`, {
    headers,
  }).then((res) => res.json())

  if (!Array.isArray(releases)) {
    throw new Error(
      `Unexpected GitHub releases response (expected an array). ` +
        `This is usually a rate-limited 403 when no GITHUB_TOKEN is set. ` +
        `Response: ${JSON.stringify(releases)}`,
    )
  }

  const changelog = releases
    .filter(({ name }) => name?.startsWith(packageName))
    .map((release) => {
      const body = release.body
        .replace(/&#39;/g, "'")
        .replace(/@([a-zA-Z0-9_-]+)(?=(,| ))/g, '[@$1](https://github.com/$1)')
      return `## [${release.tag_name}](${release.html_url})

${new Date(release.published_at).toLocaleDateString(locale)}

${body}`
    })
    .join('\n\n')

  const outDir = join(process.cwd(), 'pages', 'changelog')
  await mkdir(outDir, { recursive: true })

  await writeFile(
    join(outDir, 'index.md'),
    `---
title: 'Changelog'
---

# @napi-rs/image

${changelog}
`,
  )
}

if (import.meta.url === `file://${process.argv[1]}`) await generateChangelog()
