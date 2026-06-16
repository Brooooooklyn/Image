export default function HeroCodeSample({ html }: { html: string }) {
  return (
    <div
      className="overflow-x-auto rounded-lg border border-white/10 bg-black/30 p-4 text-sm [&_pre]:bg-transparent!"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}
