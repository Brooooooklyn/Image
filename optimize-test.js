const { readFileSync, writeFileSync } = require('fs')

const { losslessCompressPng, compressJpeg, pngQuantize } = require('./packages/binding')

const PNG = readFileSync('./un-optimized.png')

writeFileSync('optimized-lossless.png', losslessCompressPng(PNG))

writeFileSync('quantized.png', pngQuantize(PNG))

writeFileSync('optimized-lossless.jpg', compressJpeg(readFileSync('./un-optimized.jpg')))
