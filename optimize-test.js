const { readFileSync, writeFileSync } = require('fs')

const { losslessCompressPng, compressJpeg } = require('./packages/binding')

writeFileSync('optimized-lossless.png', losslessCompressPng(readFileSync('./un-optimized.png')))

writeFileSync('optimized-lossless.jpg', compressJpeg(readFileSync('./un-optimized.jpg')))
