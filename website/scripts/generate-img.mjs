import { execSync } from 'node:child_process'
import { promises as fs } from 'node:fs'
import { join } from 'node:path'

const REPO_ROOT = join(process.cwd(), '..')
const IMG_DIR = join(process.cwd(), 'public', 'img')

export async function generateDemoImages() {
  await fs.mkdir(IMG_DIR, { recursive: true })

  await fs.writeFile(join(IMG_DIR, 'example.mjs'), await fs.readFile(join(REPO_ROOT, 'example.mjs')))

  await fs.writeFile(join(IMG_DIR, 'sharp.mjs'), await fs.readFile(join(REPO_ROOT, 'sharp.mjs')))

  execSync('node example.mjs', {
    cwd: IMG_DIR,
    stdio: 'inherit',
  })

  execSync('node sharp.mjs', {
    cwd: IMG_DIR,
    stdio: 'inherit',
  })

  execSync('node manipulate.mjs', {
    cwd: IMG_DIR,
    stdio: 'inherit',
  })
}

if (import.meta.url === `file://${process.argv[1]}`) await generateDemoImages()
