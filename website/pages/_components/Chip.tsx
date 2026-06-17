import type { ReactNode } from 'react'

const cx = (...c: (string | false | undefined)[]) => c.filter(Boolean).join(' ')

const base = 'inline-flex items-center rounded-full px-2.5 py-0.5 font-mono text-xs'

const tones = {
  accent: 'bg-(--color-accent-muted) text-(--color-accent-strong)',
  muted: 'border border-(--color-border) text-(--color-muted)',
}

export default function Chip({
  children,
  tone = 'accent',
  className,
}: {
  children: ReactNode
  tone?: 'accent' | 'muted'
  className?: string
}) {
  return <span className={cx(base, tones[tone], className)}>{children}</span>
}
