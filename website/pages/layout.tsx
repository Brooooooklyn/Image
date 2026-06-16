import { useEffect, type ReactNode } from 'react'
import { useRouter } from '@void/react'
import '../app.css'
import Footer from './_components/Footer'

const GA_ID = 'G-50ZQKJLY5K'

export default function Layout({ children }: { children: ReactNode }) {
  const router = useRouter()
  const path = router.path

  useEffect(() => {
    // Client-only. NEVER render GA into SSR HTML — /playground is a COEP-isolated
    // island page and its layout never hydrates, so this effect never runs there;
    // the window.location guard is belt-and-suspenders for any future hydrating route.
    if (typeof window === 'undefined') return
    if (window.location.pathname.startsWith('/playground')) return

    if (!document.getElementById('ga-src')) {
      window.dataLayer = window.dataLayer || []
      window.gtag = function gtag() {
        // GA's snippet pushes the arguments object itself.
        window.dataLayer.push(Array.prototype.slice.call(arguments) as unknown[])
      }
      window.gtag('js', new Date())
      window.gtag('config', GA_ID, { send_page_view: false })
      const s = document.createElement('script')
      s.id = 'ga-src'
      s.async = true
      s.src = `https://www.googletagmanager.com/gtag/js?id=${GA_ID}`
      document.head.appendChild(s)
    }

    if (typeof window.gtag === 'function') {
      window.gtag('event', 'page_view', {
        page_path: window.location.pathname + window.location.search,
        page_location: window.location.href,
        page_title: document.title,
      })
    }
  }, [path])

  return (
    <div className="min-h-screen">
      <header className="flex items-center justify-between border-b border-white/10 px-6 py-4">
        <a href="/" className="font-mono font-bold">@napi-rs/image</a>
        <nav className="flex gap-4 text-sm text-(--color-muted)">
          <a href="/playground" className="hover:text-(--color-fg)">Playground</a>
          <a href="/docs" className="hover:text-(--color-fg)">Docs</a>
          <a href="/changelog" className="hover:text-(--color-fg)">Changelog</a>
          <a href="https://github.com/Brooooooklyn/Image" className="hover:text-(--color-fg)">GitHub</a>
        </nav>
      </header>
      <main>{children}</main>
      <Footer />
    </div>
  )
}
