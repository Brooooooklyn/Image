import { useSSG } from 'nextra/ssg'
import { MDXRemote } from 'next-mdx-remote'

export const getChangelog = async (packageName, locale = 'en') => {
  const releases = await fetch(
    `https://api.github.com/repos/Brooooooklyn/Image/releases?per_page=100`,
    {
      headers: {
        Authorization: `token ${process.env.GITHUB_TOKEN}`,
      },
    },
  ).then((res) => res.json())

  return {
    props: {
      ssg: releases
        .filter(({ name }) => name?.startsWith(packageName))
        .map((release) => {
          const body = release.body
            .replace(/&#39;/g, "'")
            .replace(
              /@([a-zA-Z0-9_-]+)(?=(,| ))/g,
              '[@$1](https://github.com/$1)',
            )
            .replace(
              /https:\/\/github\.com\/(\S+\/\S+\/pull\/\d+)/g, 
              `<a 
                href="$&"
                target="_blank"
                rel="noopener" 
                className="nx-text-primary-600 nx-underline nx-decoration-from-font [text-underline-position:from-font]">$1</a>`)
            .replace(
              /https:\/\/github\.com\/(.+?)\/(.+?)\/compare\/(.+?)@(.+?)\.\.\.(.+?)@(.+?)$/g,
              `<a 
              href="$&"
              target="_blank"
              rel="noopener" 
              className="nx-text-primary-600">$3@$4...$5@$6</a>`
            )
          return `## <a href="${
            release.html_url
          }" target="_blank" rel="noopener">${release.tag_name}</a> 
  ${new Date(release.published_at).toLocaleDateString(locale)} \n${body}`
        })
        .join('\n\n'),
    },
  }
}

export function Changelog() {
  const releases = useSSG()
  return <MDXRemote {...releases} />
}