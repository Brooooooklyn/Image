import { useRouter } from 'next/router'
import Script from 'next/script'
import { useConfig } from 'nextra-theme-docs'

/**
 * @type {import('nextra-theme-docs').DocsThemeConfig}
 */
export default {
  docsRepositoryBase: 'https://github.com/Brooooooklyn/Image/blob/main/website/',
  project: {
    link: 'https://github.com/Brooooooklyn/Image',
  },
  chat: {
    link: 'https://discord.gg/w8DAD7auZc',
  },
  useNextSeoProps() {
    const { asPath } = useRouter()
    if (asPath !== '/') {
      return {
        titleTemplate: '%s – Image',
      }
    }
  },
  logo: () => {
    return (
      <>
        <img src="/img/favicon.png" width={32} />
        <span style={{ width: 170 }} className="nx-mx-2 nx-font-extrabold nx-md:inline nx-select-none">
          @napi-rs/image
        </span>
      </>
    )
  },
  head: () => {
    const { title, description } = useConfig()

    return (
      <>
        {/* Favicons, meta */}
        <meta name="twitter:card" content="summary_large_image" />
        <meta name="twitter:site" content="@Brooooook_lyn" />
        <meta name="twitter:creator" content="@Brooooook_lyn" />
        <link rel="apple-touch-icon" sizes="180x180" href="/img/apple-touch-icon.png" />
        <link rel="icon" type="image/png" sizes="32x32" href="/img/favicon-32x32.png" />
        <link rel="icon" type="image/png" sizes="16x16" href="/img/favicon-16x16.png" />
        <meta name="msapplication-TileColor" content="#ffffff" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <meta httpEquiv="Content-Language" content="en" />
        <meta name="description" content={description || 'Fast image processing library'} />
        <meta property="og:title" content={title} />
        <meta
          property="og:image"
          content={`https://${
            process.env.VERCEL_URL && process.env.VERCEL_ENV !== 'production' ? process.env.VERCEL_URL : 'image.napi.rs'
          }/img/og.png`}
        />
        <meta property="og:description" content={description || 'Fast image processing library'} />
        <meta property="og:url" content="https://image.napi.rs" />
        <meta property="og:site_name" content="Image" />
        <meta property="og:type" content="website" />
        <Script src="https://www.googletagmanager.com/gtag/js?id=G-50ZQKJLY5K" />
        <Script
          dangerouslySetInnerHTML={{
            __html: `window.dataLayer = window.dataLayer || [];function gtag(){dataLayer.push(arguments);}gtag('js', new Date());gtag('config', 'G-50ZQKJLY5K');`,
          }}
        />
      </>
    )
  },
  editLink: {
    text: ({ locale }) => {
      switch (locale) {
        case 'cn':
          return '在 GitHub 上编辑本页 →'
        default:
          return 'Edit this page on GitHub →'
      }
    },
  },
  footer: {
    text: () => {
      return (
        <p>
          <a href="https://vercel.com?utm_source=napi-rs&utm_campaign=oss">
            <img src="/img/powered-by-vercel.svg" />
          </a>
          &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; Powered by{' '}
          <a
            href="https://nextra.vercel.app"
            className="nx-text-primary-600 nx-underline nx-decoration-from-font [text-underline-position:from-font]"
            target="_blank"
          >
            Nextra
          </a>
        </p>
      )
    },
  },
}
