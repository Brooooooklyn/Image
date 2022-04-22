import { execSync } from 'child_process'
import { join } from 'path'
import { promises as fs } from 'fs'
import { fileURLToPath } from 'url'

import chalk from 'chalk'
import fetch from 'node-fetch'

const __dirname = join(fileURLToPath(import.meta.url), '..', 'public', 'img')

await fs.writeFile('public/img/example.mjs', await fs.readFile('../example.mjs'))

await fs.writeFile('public/img/sharp.mjs', await fs.readFile('../sharp.mjs'))

if (process.env.VERCEL) {
  const gnuBinary = await fetch(`https://unpkg.com/@napi-rs/image-linux-x64-gnu`, {
    follow: 10,
  }).then((res) => res.arrayBuffer())
  console.info(chalk.greenBright(`Installed @napi-rs/image.linux-x64-gnu, size: ${gnuBinary.byteLength}`))
  await fs.writeFile(join(__dirname, '../../../packages/binding/image.linux-x64-gnu.node'), Buffer.from(gnuBinary))
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
