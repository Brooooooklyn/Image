import { execSync } from 'child_process'
import { join } from 'path'
import { promises as fs } from 'fs'
import { fileURLToPath } from 'url'

const __dirname = join(fileURLToPath(import.meta.url), '..', 'public', 'img')

await fs.writeFile('public/img/example.mjs', await fs.readFile('../example.mjs'))

await fs.writeFile('public/img/sharp.mjs', await fs.readFile('../sharp.mjs'))

execSync('node example.mjs', {
  cwd: __dirname,
  stdio: 'inherit',
})

execSync('node sharp.mjs', {
  cwd: __dirname,
  stdio: 'inherit',
})
