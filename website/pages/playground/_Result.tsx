// website/pages/playground/_Result.tsx
// Result panel: before/after compare, size savings, download, code snippet.
import { useState } from 'react'
import BeforeAfter from '../_components/_BeforeAfter'
import { snippetFor } from './_snippet'
import type { Op } from './protocol'

const kb = (n: number) => `${(n / 1024).toFixed(1)} KB`

function extFor(outFormat: string): string {
  if (outFormat === 'webpLossless') return 'webp'
  if (outFormat === 'jpeg') return 'jpg'
  return outFormat
}

export default function Result({
  originalUrl,
  originalBytes,
  result,
  op,
  inputFormat,
}: {
  originalUrl: string
  originalBytes: number
  result: { url: string | null; bytes: number; outFormat: string }
  op: Op
  inputFormat?: string
}) {
  const [copied, setCopied] = useState(false)

  const pct = Math.round((1 - result.bytes / originalBytes) * 100)
  const snippet = snippetFor(op, inputFormat)

  const handleCopy = () => {
    navigator.clipboard.writeText(snippet).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    })
  }

  return (
    <div data-testid="pg-result" className="flex flex-col gap-6">
      {/* Before/after compare or unavailable notice */}
      {result.url != null ? (
        <BeforeAfter
          before={originalUrl}
          after={result.url}
          beforeLabel="original"
          afterLabel={result.outFormat}
        />
      ) : (
        <div className="flex aspect-[3/2] w-full items-center justify-center rounded-lg border border-white/10 bg-white/5 text-sm text-(--color-muted)">
          Preview not available for this format — download to view.
        </div>
      )}

      {/* Size row */}
      <div className="flex items-center gap-3 text-sm">
        <span className="text-(--color-muted)">Size:</span>
        <span className="text-(--color-fg)">{kb(originalBytes)}</span>
        <span className="text-(--color-muted)">→</span>
        <span data-testid="pg-bytes" className="text-(--color-fg)">
          {kb(result.bytes)} ({result.bytes} bytes)
        </span>
        {pct > 0 ? (
          <span className="font-semibold text-(--color-accent)">−{pct}%</span>
        ) : (
          <span className="font-semibold text-(--color-muted)">+{Math.abs(pct)}%</span>
        )}
      </div>

      {/* Download */}
      {result.url != null && (
        <a
          href={result.url}
          download={`output.${extFor(result.outFormat)}`}
          className="inline-flex w-fit items-center gap-2 rounded border border-white/10 bg-white/5 px-4 py-2 text-sm text-(--color-fg) hover:bg-white/10 hover:text-(--color-accent)"
        >
          Download {result.outFormat === 'webpLossless' ? 'webp' : result.outFormat.toUpperCase()}
        </a>
      )}

      {/* Code snippet */}
      <div className="relative rounded-lg border border-white/10 bg-white/5">
        <div className="flex items-center justify-between border-b border-white/10 px-4 py-2">
          <span className="text-xs text-(--color-muted)">Code</span>
          <button
            onClick={handleCopy}
            className="rounded border border-white/15 px-2 py-1 text-xs text-(--color-muted) hover:text-(--color-fg)"
            aria-label="Copy code snippet"
          >
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
        <pre className="overflow-x-auto p-4 text-xs text-(--color-fg) [&_*]:bg-transparent!">{snippet}</pre>
      </div>
    </div>
  )
}
