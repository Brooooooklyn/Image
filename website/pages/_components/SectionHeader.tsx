import type { ReactNode } from 'react'

const cx = (...c: (string | false | undefined)[]) => c.filter(Boolean).join(' ')

export default function SectionHeader({
  index,
  label,
  title,
  subhead,
  align = 'left',
}: {
  index?: string
  label: string
  title: ReactNode
  subhead?: ReactNode
  align?: 'left' | 'center'
}) {
  const centered = align === 'center'
  return (
    <div className={cx('flex flex-col', centered && 'items-center text-center')}>
      <span className="eyebrow">{index ? `${index} — ${label}` : label}</span>
      <h2 className="mt-4 font-display text-h2 text-(--color-fg)">{title}</h2>
      {subhead ? (
        <p className={cx('mt-4 max-w-2xl text-(--color-muted)', centered && 'mx-auto')}>{subhead}</p>
      ) : null}
    </div>
  )
}
