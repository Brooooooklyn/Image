import 'nextra-theme-docs/style.css'

import '../style.css'

export default function Nextra({ Component, pageProps }) {
  // Use the layout defined at the page level, if available
  const getLayout = Component.getLayout || ((page) => page)

  return getLayout(<Component {...pageProps} />)
}
