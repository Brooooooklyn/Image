import nextra from 'nextra'

const withNextra = nextra({
  staticImage: false,
  latex: true,
  search: {
    codeblocks: false,
  },
  defaultShowCopyCode: true,
})

export default withNextra({
  images: {
    unoptimized: true,
  },
})
