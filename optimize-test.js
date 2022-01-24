const { readFileSync, writeFileSync } = require('fs')

const { losslessCompressPng, compressJpeg, pngQuantize, svgMin } = require('./packages/binding')

const PNG = readFileSync('./un-optimized.png')
const SVG = `<?xml version="1.0" encoding="iso-8859-1"?>
<!-- Generator: Adobe Illustrator 16.0.0, SVG Export Plug-In . SVG Version: 6.00 Build 0)  -->
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg version="1.1" id="Capa_1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" x="0px" y="0px"
	 width="489.589px" height="489.589px" viewBox="0 0 489.589 489.589" style="enable-background:new 0 0 489.589 489.589;"
	 xml:space="preserve">
<g>
	<g>
		<path d="M484.536,30.412c-1.034,3.078-2.545,6.233-4.652,9.279c-7.39,10.654-18.507,15.733-24.838,11.343
			c-3.896-2.707-5.046-8.383-3.635-14.827c-6.448,1.404-12.127,0.258-14.836-3.645c-4.384-6.324,0.701-17.44,11.35-24.832
			c3.041-2.108,6.199-3.625,9.28-4.652l-1.698-1.687L427.246,0l-10.531,10.545c0.204,12.864-5.843,27.475-17.549,39.17
			c-11.698,11.7-26.305,17.751-39.177,17.544l-0.705,0.709c7.942,9.993,5.113,26.908-6.645,39.649l-97.701,99.254
			c-16.212-1.154-40.134-7.153-48.211-15.222c-15.333-15.333-3.178-38.175,25.773-48.339c12.693-4.462,31.346-12.728,48.71-17.909
			l-95.248-0.21l-25.812,109.717l-79.421-17.376L0.782,363.311l90.57-47.548l-1.03,1.026c3.675,1.671,7.206,3.45,10.614,5.289
			c-7.065,8.54-6.696,21.16,1.305,29.154l36.904,36.9c8.421,8.432,22.019,8.464,30.529,0.185c3.274,5.55,4.829,8.984,4.829,8.984
			l2.136-2.132l-49.56,94.419l145.77-79.928l-17.895-81.815l108.21-25.455l-0.204-94.904c-5.186,17.288-13.397,35.814-17.833,48.445
			c-10.167,28.95-33.014,41.093-48.338,25.776c-7.979-7.987-13.914-31.387-15.156-47.634l99.048-100.611
			c12.819-11.066,29.378-13.393,38.992-5.171c-0.673-13.188,5.422-28.427,17.532-40.539c12.712-12.711,28.85-18.755,42.467-17.334
			l9.137-9.137L487.43,33.31L484.536,30.412z"/>
	</g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
<g>
</g>
</svg>
`

writeFileSync('optimized-lossless.png', losslessCompressPng(PNG))

writeFileSync('quantized.png', pngQuantize(PNG))

writeFileSync('optimized-lossless.jpg', compressJpeg(readFileSync('./un-optimized.jpg')))

writeFileSync('optimized.svg', svgMin(SVG))
