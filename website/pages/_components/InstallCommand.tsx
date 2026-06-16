import CopyButton from './_CopyButton' with { island: 'visible' }

const CMD = 'npm install @napi-rs/image'

export default function InstallCommand() {
  return (
    <div className="mx-auto mt-8 flex max-w-md items-center justify-between gap-3 rounded-lg border border-white/10 bg-white/5 px-4 py-3 font-mono text-sm">
      <code>{CMD}</code>
      <CopyButton text={CMD} />
    </div>
  )
}
