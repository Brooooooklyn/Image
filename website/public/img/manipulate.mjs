import { promises as fs } from 'fs'

import { Transformer } from '@napi-rs/image'
import chalk from 'chalk'

const PNG = await fs.readFile('./un-optimized.png')
const TAJ_ORIG = await fs.readFile('./taj_orig.jpg')

try {
  await fs.writeFile('grayscale.manipulated.webp', await new Transformer(PNG).grayscale().webp())

  console.info(chalk.cyanBright(`Grayscale PNG done`))

  await fs.writeFile('invert.manipulated.webp', await new Transformer(PNG).invert().webp())

  console.info(chalk.cyanBright(`Invert PNG done`))

  await fs.writeFile('blur.manipulated.webp', await new Transformer(PNG).blur(10).webp())

  console.info(chalk.cyanBright(`Blur PNG done`))

  await fs.writeFile('unsharpen.manipulated.webp', await new Transformer(TAJ_ORIG).unsharpen(10, 10).webp())

  console.info(chalk.cyanBright(`Unsharpen PNG done`))

  // https://en.wikipedia.org/wiki/Kernel_(image_processing)#Details
  // Sharpen:
  await fs.writeFile(
    'filter3x3.manipulated.webp',
    await new Transformer(PNG).filter3x3([0, -1, 0, -1, 5, -1, 0, -1, 0]).webp(),
  )

  console.info(chalk.cyanBright(`filter3x3 PNG done`))

  await fs.writeFile('contrast.manipulated.webp', await new Transformer(PNG).adjustContrast(50).webp())

  console.info(chalk.cyanBright(`AdjustContrast PNG done`))

  await fs.writeFile('brighten.manipulated.webp', await new Transformer(PNG).brighten(30).webp())

  console.info(chalk.cyanBright(`Brighten PNG done`))

  await fs.writeFile('huerotate.manipulated.webp', await new Transformer(PNG).huerotate(90).webp())

  console.info(chalk.cyanBright(`Huerotate PNG done`))

  await fs.writeFile('crop.manipulated.webp', await new Transformer(PNG).crop(270, 40, 500, 500).webp())

  console.info(chalk.cyanBright(`Crop PNG done`))
} catch (e) {
  console.error(e)
}
