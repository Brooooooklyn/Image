const { readFileSync, writeFileSync } = require('fs')

const { losslessCompressPng, compressJpeg } = require('./index')

writeFileSync('optimized-lossless.png', losslessCompressPng(readFileSync('./un-optimized.png')))

writeFileSync('optimized-lossless.jpg', compressJpeg(readFileSync('./un-optimized.jpg')))
