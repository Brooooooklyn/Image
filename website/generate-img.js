import { execSync } from 'node:child_process'
import { join } from 'node:path'
import { promises as fs } from 'node:fs'
import { fileURLToPath } from 'node:url'

import chalk from 'chalk'
import fetch from 'node-fetch'

const __dirname = join(fileURLToPath(import.meta.url), '..', 'public', 'img')

await fs.writeFile('public/img/example.mjs', await fs.readFile('../example.mjs'))

await fs.writeFile('public/img/sharp.mjs', await fs.readFile('../sharp.mjs'))

if (process.env.VERCEL) {
  const arch = process.arch
  const gnuBinary = await fetch(`https://unpkg.com/@napi-rs/image-linux-${arch}-gnu`, {
    redirect: 'follow',
    follow: 10,
  }).then((res) => res.arrayBuffer())
  console.info(chalk.greenBright(`Installed @napi-rs/image.linux-${arch}-gnu, size: ${gnuBinary.byteLength}`))
  await fs.writeFile(join(__dirname, `../../../packages/binding/image.linux-${arch}-gnu.node`), Buffer.from(gnuBinary))
}

execSync('node example.mjs', {
  cwd: __dirname,
  stdio: 'inherit',
})

execSync('node sharp.mjs', {
  cwd: __dirname,
  stdio: 'inherit',
})

execSync('node manipulate.mjs', {
  cwd: __dirname,
  stdio: 'inherit',
})

execSync(`node og-image`, {
  cwd: join(__dirname, '..', '..'),
  stdio: 'inherit',
})
