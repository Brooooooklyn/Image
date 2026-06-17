import type { AnchorHTMLAttributes, ReactNode } from 'react'

const cx = (...c: (string | false | undefined)[]) => c.filter(Boolean).join(' ')

const base =
  'inline-flex items-center justify-center gap-2 rounded-lg px-5 py-2.5 text-sm font-medium transition-colors'

const variants = {
  primary: 'bg-(--color-accent) text-(--color-accent-fg) hover:bg-(--color-accent-strong)',
  secondary: 'border border-(--color-border-strong) text-(--color-fg) hover:bg-(--color-surface-1)',
  ghost: 'text-(--color-muted) hover:text-(--color-fg)',
}

export default function Button({
  variant = 'primary',
  className,
  children,
  ...rest
}: AnchorHTMLAttributes<HTMLAnchorElement> & {
  variant?: 'primary' | 'secondary' | 'ghost'
  children?: ReactNode
}) {
  return (
    <a className={cx(base, variants[variant], className)} {...rest}>
      {children}
    </a>
  )
}
