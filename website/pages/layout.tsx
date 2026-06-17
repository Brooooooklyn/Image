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
    <>
      {/* Mark JS-capable clients before first paint so scroll-reveal hidden state
          (gated behind html.js in app.css) only applies when JS can reveal it —
          no-JS / crawler HTML stays fully visible. */}
      <script dangerouslySetInnerHTML={{ __html: "document.documentElement.classList.add('js')" }} />
      <div className="min-h-screen">
        <header className="site-header sticky top-0 z-50 border-b border-(--color-border)">
          <div className="container-page flex h-14 items-center justify-between">
            <a href="/" className="font-mono text-sm font-medium tracking-tight text-(--color-fg)">@napi-rs/image</a>
            <nav className="flex items-center gap-6 text-sm text-(--color-muted)">
              <a href="/playground" className="transition-colors hover:text-(--color-fg)">Playground</a>
              <a href="/docs" className="transition-colors hover:text-(--color-fg)">Docs</a>
              <a href="/changelog" className="transition-colors hover:text-(--color-fg)">Changelog</a>
              <a href="https://github.com/Brooooooklyn/Image" className="transition-colors hover:text-(--color-fg)">GitHub</a>
            </nav>
          </div>
        </header>
        <main>{children}</main>
        <Footer />
      </div>
    </>
  )
}
