import Markdown from 'markdown-to-jsx'
import { useSSG } from 'nextra/ssg'

export const getChangelog = async () => {
  const releases = await fetch(`https://api.github.com/repos/Brooooooklyn/Image/releases?per_page=100`, {
    headers: {
      Authorization: `token ${process.env.GITHUB_TOKEN}`,
    },
  }).then((res) => res.json())

  return {
    props: {
      ssg: releases,
    },
  }
}

export function Changelog({ locale = 'en', packageName }) {
  const releases = useSSG()
  return (
    <Markdown>
      {releases
        .filter(({ name }) => name?.startsWith(packageName))
        .map((release) => {
          const body = release.body
            .replace(/&#39;/g, "'")
            .replace(
              /@([a-zA-Z0-9_-]+)(?=(,| ))/g,
              '<a href="https://github.com/$1" target="_blank" rel="noopener">@$1</a>',
            )
          return `## <a href="${release.html_url}" target="_blank" rel="noopener">${release.tag_name}</a> 
${new Date(release.published_at).toLocaleDateString(locale)} \n${body}`
        })
        .join('\n\n')}
    </Markdown>
  )
}
