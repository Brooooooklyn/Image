import { useEffect, useRef, useState } from 'react'

const cx = (...c: (string | false | undefined)[]) => c.filter(Boolean).join(' ')

export default function CountUp({
  to,
  decimals,
  prefix,
  suffix,
  duration,
  className,
}: {
  to: number
  decimals?: number
  prefix?: string
  suffix?: string
  duration?: number
  className?: string
}) {
  const format = (n: number) => (prefix ?? '') + n.toFixed(decimals ?? 0) + (suffix ?? '')
  const [val, setVal] = useState(to)
  const ref = useRef<HTMLSpanElement>(null)

  useEffect(() => {
    if (typeof window === 'undefined' || typeof IntersectionObserver === 'undefined') return
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return

    const el = ref.current
    if (!el) return

    setVal(0)
    let raf = 0
    let start = 0
    const total = duration ?? 1100

    const obs = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue
          obs.disconnect()
          const tick = (now: number) => {
            if (!start) start = now
            const t = Math.min(1, (now - start) / total)
            const eased = 1 - Math.pow(1 - t, 3)
            if (t < 1) {
              setVal(to * eased)
              raf = requestAnimationFrame(tick)
            } else {
              setVal(to)
            }
          }
          raf = requestAnimationFrame(tick)
        }
      },
      { threshold: 0.4 },
    )
    obs.observe(el)
    return () => {
      obs.disconnect()
      cancelAnimationFrame(raf)
    }
  }, [to, duration])

  return (
    <span ref={ref} className={cx('tabular-nums', className)}>
      {format(val)}
    </span>
  )
}
