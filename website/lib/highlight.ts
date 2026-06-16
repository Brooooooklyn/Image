import { codeToHtml } from 'shiki'

export function highlight(code: string, lang = 'ts') {
  return codeToHtml(code, { lang, theme: 'github-dark' })
}
