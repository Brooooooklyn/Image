import type { ElementType, ReactNode } from 'react'
import { useEffect, useRef } from 'react'

const cx = (...c: (string | false | undefined)[]) => c.filter(Boolean).join(' ')

export default function Reveal({
  children,
  className,
  delay,
  as,
}: {
  children: ReactNode
  className?: string
  delay?: number
  as?: ElementType
}) {
  const Tag = as ?? 'div'
  const ref = useRef<HTMLElement>(null)

  useEffect(() => {
    const el = ref.current
    if (!el || typeof IntersectionObserver === 'undefined') {
      el?.classList.add('is-visible')
      return
    }
    const obs = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add('is-visible')
            obs.unobserve(entry.target)
          }
        }
      },
      { threshold: 0.12 },
    )
    obs.observe(el)
    return () => obs.disconnect()
  }, [])

  return (
    <Tag ref={ref} className={cx('reveal', className)} style={{ transitionDelay: delay ? `${delay}ms` : undefined }}>
      {children}
    </Tag>
  )
}
