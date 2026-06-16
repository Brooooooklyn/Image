import { useRef, useState } from 'react'

export default function BeforeAfter({
  before,
  after,
  beforeLabel,
  afterLabel,
}: {
  before: string
  after: string
  beforeLabel?: string
  afterLabel?: string
}) {
  const [pos, setPos] = useState(50)
  const ref = useRef<HTMLDivElement>(null)

  const move = (clientX: number) => {
    const el = ref.current
    if (!el) return
    const r = el.getBoundingClientRect()
    setPos(Math.min(100, Math.max(0, ((clientX - r.left) / r.width) * 100)))
  }

  return (
    <div
      ref={ref}
      className="relative aspect-[3/2] w-full select-none overflow-hidden rounded-lg border border-white/10"
      onPointerMove={(e) => e.buttons === 1 && move(e.clientX)}
      onPointerDown={(e) => move(e.clientX)}
    >
      <img
        src={after}
        alt={afterLabel ?? 'after'}
        className="absolute inset-0 h-full w-full object-cover"
        loading="lazy"
      />
      <div className="absolute inset-0 overflow-hidden" style={{ width: `${pos}%` }}>
        <img
          src={before}
          alt={beforeLabel ?? 'before'}
          className="absolute inset-0 h-full w-full max-w-none object-cover"
          style={{ width: ref.current?.clientWidth }}
          loading="lazy"
        />
      </div>
      <div
        role="slider"
        aria-valuenow={Math.round(pos)}
        aria-valuemin={0}
        aria-valuemax={100}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft') setPos((p) => Math.max(0, p - 2))
          if (e.key === 'ArrowRight') setPos((p) => Math.min(100, p + 2))
        }}
        className="absolute top-0 bottom-0 w-0.5 cursor-ew-resize bg-(--color-accent)"
        style={{ left: `${pos}%` }}
      />
    </div>
  )
}
