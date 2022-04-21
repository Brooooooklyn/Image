import nextra from 'nextra'

const withNextra = nextra({
  theme: 'nextra-theme-docs',
  themeConfig: './nextra.config.js',
  unstable_flexsearch: true,
  unstable_staticImage: true,
})

export default withNextra({
  experiments: {
    esmExternals: true,
  },
})
