import { useEffect, type ReactNode } from 'react'
import { useRouter } from '@void/react'
import Footer from './_components/Footer'
import '../app.css'

const GA_ID = 'G-50ZQKJLY5K'

export default function Layout({ children }: { children: ReactNode }) {
  const router = useRouter()
  const path = router.path
  const analyticsEnabled = !path.startsWith('/playground')

  useEffect(() => {
    if (!analyticsEnabled || typeof window === 'undefined' || typeof window.gtag !== 'function') return
    window.gtag('event', 'page_view', {
      page_path: window.location.pathname + window.location.search,
      page_location: window.location.href,
      page_title: document.title,
    })
  }, [path, analyticsEnabled])

  return (
    <div className="min-h-screen">
      {analyticsEnabled && (
        <>
          <script async src={`https://www.googletagmanager.com/gtag/js?id=${GA_ID}`} />
          <script
            dangerouslySetInnerHTML={{
              __html: `window.dataLayer=window.dataLayer||[];function gtag(){dataLayer.push(arguments);}gtag('js',new Date());gtag('config','${GA_ID}',{send_page_view:false});`,
            }}
          />
        </>
      )}
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
