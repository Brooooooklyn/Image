import { promises as fs } from 'fs'

import { createCanvas, GlobalFonts, Image } from '@napi-rs/canvas'
import { pngQuantize } from '@napi-rs/image'
import fetch from 'node-fetch'

const canvas = createCanvas(1200, 700)
const ctx = canvas.getContext('2d')

ctx.globalCompositeOperation = 'destination-over'

const FONT_URL = `https://github.com/Brooooooklyn/canvas/raw/main/__test__/fonts/iosevka-slab-regular.ttf`

if (!GlobalFonts.families.some(({ family }) => family === 'Iosevka Slab')) {
  const font = await fetch(FONT_URL, {
    redirect: 'follow',
    follow: 10,
  }).then((res) => res.arrayBuffer())
  GlobalFonts.register(Buffer.from(font))
}

ctx.fillStyle = 'white'
ctx.font = '48px Iosevka Slab'
const Title = '@napi-rs/image'
ctx.fillText(Title, 80, 100)

const Arrow = new Image()
Arrow.src = Buffer.from(`
<svg viewBox="0 0 1088 615" version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
  <g stroke-width="1" fill="none" fill-rule="evenodd">
      <text font-family="'Iosevka Slab', sans-serif" font-size="18" font-weight="600" fill="#00e676">
          <tspan x="900" y="459">Optimized Images</tspan>
      </text>
      <g transform="translate(1002, 326)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-1"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="84" height="84" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="18.891" y="46.7096774">.png</tspan>
          </text>
      </g>
      <g transform="translate(1002, 214)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-2"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="84" height="84" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="15" y="46.7096774">.avif</tspan>
          </text>
      </g>
      <g transform="translate(894, 326)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-3"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="84" height="84" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="21.817" y="46.7096774">.jpg</tspan>
          </text>
      </g>
      <g transform="translate(894, 214)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-4"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="84" height="84" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="9" y="46.7096774">.webp</tspan>
          </text>
      </g>
      <g transform="translate(342, 225)" stroke="#73e8ff" stroke-width="4">
          <path d="M499.558824,86.52 C499.558824,86.52 484.852941,81.02 439.908088,109.436667 C394.963235,137.853333 380.992647,164.436667 380.992647,164.436667" stroke-dasharray="7"></path>
          <path d="M499.558824,86.0616667 C499.558824,86.0616667 484.852941,91.5616667 439.908088,63.145 C394.963235,34.7283333 380.992647,8.145 380.992647,8.145" stroke-dasharray="7"></path>
          <path d="M0.477941176,170.395 C0.477941176,170.395 169.382939,98.895 447.847936,98.895" stroke-dasharray="6"></path>
          <path d="M0.477941176,72.395 C0.477941176,72.395 169.382939,0.895 447.847936,0.895" stroke-dasharray="6" transform="translate(224.162939, 36.645000) scale(1, -1) translate(-224.162939, -36.645000) "></path>
      </g>
      <text font-family="'Iosevka Slab', sans-serif" font-size="18" font-weight="600" fill="#ffcc80">
          <tspan x="24.934" y="562">Raw Images</tspan>
      </text>
      <g transform="translate(228, 335)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-5"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.jpg</tspan>
          </text>
      </g>
      <g transform="translate(228, 223)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-6"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.png</tspan>
          </text>
      </g>
      <g transform="translate(116, 391)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-7"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.tiff</tspan>
          </text>
      </g>
      <g transform="translate(116, 279)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-8"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.ico</tspan>
          </text>
      </g>
      <g transform="translate(116, 167)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-9"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.pnm</tspan>
          </text>
      </g>
      <g transform="translate(4, 447)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-10"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.bmp</tspan>
          </text>
      </g>
      <g transform="translate(4, 335)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-11"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.tga</tspan>
          </text>
      </g>
      <g transform="translate(4, 223)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-12"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.hdr</tspan>
          </text>
      </g>
      <g transform="translate(4, 111)">
          <g>
              <use fill-opacity="0.1" fill="#b3e5fc" fill-rule="evenodd" xlink:href="#path-13"></use>
              <rect stroke="#b3e5fc" stroke-width="4" x="-2" y="-2" width="66" height="66" rx="3"></rect>
          </g>
          <text font-family="'Iosevka Slab', sans-serif" font-size="22" font-weight="500" fill="#FFFFFF">
              <tspan x="10" y="38">.dxt</tspan>
          </text>
      </g>
  </g>
</svg>`)
ctx.drawImage(Arrow, 80, 60)

const ViceCityGradient = ctx.createLinearGradient(0, 0, 1200, 0)
ViceCityGradient.addColorStop(0, '#3494e6')
ViceCityGradient.addColorStop(1, '#EC6EAD')
ctx.fillStyle = ViceCityGradient
ctx.fillRect(0, 0, 1200, 700)

fs.writeFile(
  'public/img/og.png',
  await pngQuantize(await canvas.encode('png'), {
    maxQuality: 90,
    minQuality: 75,
  }),
)
