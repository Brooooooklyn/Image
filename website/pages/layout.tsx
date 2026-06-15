import type { ReactNode } from 'react'
import '../app.css'

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <div className="min-h-screen">
      <header className="border-b border-white/10 px-6 py-4">
        <a href="/" className="font-mono font-bold">
          @napi-rs/image
        </a>
      </header>
      <main>{children}</main>
    </div>
  )
}
