import { Footer, Layout, Navbar } from 'nextra-theme-docs'
import { Head } from 'nextra/components'
import { getPageMap } from 'nextra/page-map'
import Script from 'next/script'
import 'nextra-theme-docs/style.css'
import '../style.css'

export const metadata = {
  title: '@napi-rs/image',
  description: 'Fast image processing library',
  openGraph: {
    title: '@napi-rs/image',
    description: 'Fast image processing library',
    url: 'https://image.napi.rs',
    siteName: 'Image',
    type: 'website',
    images: [
      {
        url: 'https://image.napi.rs/img/og.png',
      },
    ],
  },
  twitter: {
    card: 'summary_large_image',
    site: '@Brooooook_lyn',
    creator: '@Brooooook_lyn',
  },
  icons: {
    icon: [
      { url: '/img/favicon-32x32.png', sizes: '32x32', type: 'image/png' },
      { url: '/img/favicon-16x16.png', sizes: '16x16', type: 'image/png' },
    ],
    apple: '/img/apple-touch-icon.png',
  },
}

const navbar = (
  <Navbar
    logo={
      <>
        <img src="/img/favicon.png" width={32} alt="@napi-rs/image" />
        <span style={{ width: 170 }} className="x:mx-2 x:font-extrabold x:md:inline x:select-none">
          @napi-rs/image
        </span>
      </>
    }
    projectLink="https://github.com/Brooooooklyn/Image"
    chatLink="https://discord.gg/w8DAD7auZc"
  />
)

const footer = (
  <Footer>
    <p>
      <a href="https://vercel.com?utm_source=napi-rs&utm_campaign=oss">
        <img src="/img/powered-by-vercel.svg" alt="Powered by Vercel" />
      </a>
      &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; Powered by{' '}
      <a
        href="https://nextra.site"
        className="x:text-primary-600 x:underline x:decoration-from-font [text-underline-position:from-font]"
        target="_blank"
        rel="noreferrer"
      >
        Nextra
      </a>
    </p>
  </Footer>
)

export default async function RootLayout({ children }) {
  return (
    <html lang="en" dir="ltr" suppressHydrationWarning>
      <Head color={{ hue: 300, saturation: 100 }}>
        <meta name="msapplication-TileColor" content="#ffffff" />
        <Script src="https://www.googletagmanager.com/gtag/js?id=G-50ZQKJLY5K" strategy="afterInteractive" />
        <Script
          id="gtag-init"
          strategy="afterInteractive"
          dangerouslySetInnerHTML={{
            __html: `window.dataLayer = window.dataLayer || [];function gtag(){dataLayer.push(arguments);}gtag('js', new Date());gtag('config', 'G-50ZQKJLY5K');`,
          }}
        />
      </Head>
      <body>
        <Layout
          navbar={navbar}
          pageMap={await getPageMap()}
          docsRepositoryBase="https://github.com/Brooooooklyn/Image/blob/main/website"
          editLink="Edit this page on GitHub"
          footer={footer}
        >
          {children}
        </Layout>
      </body>
    </html>
  )
}
