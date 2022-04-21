import { promises as fs } from 'fs'
import { cpus } from 'os'
import { hrtime } from 'process'

import { from, timer, lastValueFrom, Subject } from 'rxjs'
import { mergeMap, takeUntil } from 'rxjs/operators'
import sharp from 'sharp'

import { ChromaSubsampling, Transformer } from '@napi-rs/image'

// https://github.com/ianare/exif-samples/blob/master/jpg/orientation/portrait_5.jpg
const WITH_EXIF = await fs.readFile('./with-exif.jpg')

const CPU_LENGTH = cpus().length

const DEFAULT_TOTAL_ITERATIONS = 10000
const DEFAULT_MAX_DURATION = 20000

function bench(name, options = {}) {
  const suites = []
  return {
    add(suiteName, suiteFn) {
      suites.push({
        name: suiteName,
        fn: suiteFn,
      })
      return this
    },
    run: async () => {
      let fastest = {
        perf: -1,
        name: '',
      }
      for (const { suiteName, fn: suiteFn } of suites) {
        try {
          await suiteFn()
        } catch (e) {
          console.error(`Warming up ${suiteName} failed`)
          throw e
        }
      }
      for (const { name: suiteName, fn: suiteFn } of suites) {
        const iterations = options.iterations ?? DEFAULT_TOTAL_ITERATIONS
        const parallel = options.parallel ?? CPU_LENGTH
        const maxDuration = options.maxDuration ?? DEFAULT_MAX_DURATION
        const start = hrtime.bigint()
        let totalIterations = 0
        let finishedIterations = 0
        const finish$ = new Subject()
        await lastValueFrom(
          from({ length: iterations }).pipe(
            mergeMap(async () => {
              totalIterations++
              await suiteFn()
              finishedIterations++
              if (finishedIterations === totalIterations) {
                finish$.next()
                finish$.complete()
              }
            }, parallel),
            takeUntil(timer(maxDuration)),
          ),
        )
        if (finishedIterations !== totalIterations) {
          await lastValueFrom(finish$)
        }
        const duration = Number(hrtime.bigint() - start)
        const currentPerf = totalIterations / duration
        if (currentPerf > fastest.perf) {
          fastest = {
            perf: currentPerf,
            name: suiteName,
          }
        }
        console.info(`${suiteName} ${Math.round(currentPerf * 1e9)} ops/s`)
      }
      console.info(`In ${name} suite, fastest is ${fastest.name}`)
    },
  }
}

await bench('webp')
  .add('@napi-rs/image', () =>
    new Transformer(WITH_EXIF)
      .rotate()
      .resize(450 / 2)
      .webp(75),
  )
  .add('sharp', () =>
    sharp(WITH_EXIF)
      .rotate()
      .resize(450 / 2)
      .webp({ quality: 75 })
      .toBuffer(),
  )
  .run()

bench('avif')
  .add('@napi-rs/image', () =>
    new Transformer(WITH_EXIF)
      .rotate()
      .resize(450 / 2)
      .avif({ quality: 70, chromaSubsampling: ChromaSubsampling.Yuv420 }),
  )
  .add('sharp', () =>
    sharp(WITH_EXIF)
      .rotate()
      .resize(450 / 2)
      .avif({ quality: 70, chromaSubsampling: '4:2:0' })
      .toBuffer(),
  )
  .run()
