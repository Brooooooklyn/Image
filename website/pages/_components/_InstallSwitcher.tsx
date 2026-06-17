import { useEffect, useState } from 'react'
import CopyButton from './_CopyButton'

const cx = (...c: (string | false | undefined)[]) => c.filter(Boolean).join(' ')

const PMS = [
  { id: 'vp', cmd: 'vp add @napi-rs/image' },
  { id: 'pnpm', cmd: 'pnpm add @napi-rs/image' },
  { id: 'yarn', cmd: 'yarn add @napi-rs/image' },
  { id: 'bun', cmd: 'bun add @napi-rs/image' },
  { id: 'npm', cmd: 'npm i @napi-rs/image' },
]

const STORAGE_KEY = 'napi-image:pm'

export default function InstallSwitcher() {
  const [active, setActive] = useState('vp')

  useEffect(() => {
    if (typeof window === 'undefined') return
    try {
      const saved = window.localStorage.getItem(STORAGE_KEY)
      if (saved && PMS.some((p) => p.id === saved)) setActive(saved)
    } catch {}
  }, [])

  const onSelect = (id: string) => {
    setActive(id)
    try {
      window.localStorage.setItem(STORAGE_KEY, id)
    } catch {}
  }

  const activeCmd = (PMS.find((p) => p.id === active) ?? PMS[0]).cmd

  return (
    <div className="max-w-md">
      <div className="flex items-center gap-4 font-mono text-sm">
        {PMS.map((pm) => {
          const isActive = pm.id === active
          return (
            <button
              key={pm.id}
              type="button"
              onClick={() => onSelect(pm.id)}
              className={cx(
                'relative pb-2 transition-colors',
                isActive ? 'text-(--color-accent)' : 'text-(--color-muted) hover:text-(--color-fg)',
              )}
            >
              {pm.id}
              {isActive ? (
                <span className="absolute inset-x-0 -bottom-px h-0.5 rounded-full bg-(--color-accent)" />
              ) : null}
            </button>
          )
        })}
      </div>
      <div className="mt-3 flex items-center justify-between gap-3 rounded-lg border border-(--color-border) bg-(--color-surface-1) px-4 py-3 font-mono text-sm">
        <span className="flex min-w-0 items-center gap-2">
          <span className="text-(--color-faint)">$</span>
          <span className="truncate text-(--color-fg)">{activeCmd}</span>
        </span>
        <CopyButton text={activeCmd} />
      </div>
    </div>
  )
}
