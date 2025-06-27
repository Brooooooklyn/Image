import nextra from 'nextra'

const withNextra = nextra({
  theme: 'nextra-theme-docs',
  themeConfig: './nextra.config.js',
  staticImage: false,
  latex: true,
  search: {
    codeblocks: false,
  },
  defaultShowCopyCode: true,
})

export default withNextra({
  experimental: {
    esmExternals: true,
  },
  images: {
    unoptimized: true,
  },
})
