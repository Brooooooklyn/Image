import type { FormatRow } from '../_data/formats'
import { formatRows, matrixCaption } from '../_data/formats'
import SectionHeader from './SectionHeader'
import Reveal from './_Reveal'

function SupportCell({ s, note }: { s: 'yes' | 'no'; note?: string }) {
  if (s === 'yes') {
    return (
      <span className="font-mono tabular-nums text-(--color-accent)" aria-label="supported">
        ✓
      </span>
    )
  }
  return (
    <span className="font-mono tabular-nums text-(--color-faint)" aria-label="not supported">
      ✗
    </span>
  )
}

export default function FormatMatrix() {
  return (
    <section className="border-t border-(--color-border)">
      <div className="container-page py-20 md:py-28">
        <SectionHeader
          index="03"
          label="FORMATS"
          title={<>Every format you <span className="text-(--color-accent)">need</span></>}
          subhead={matrixCaption}
        />
        <Reveal className="mt-12">
          <div className="overflow-x-auto rounded-xl border border-(--color-border) bg-(--color-surface-1)">
            <table className="w-full min-w-[34rem] border-collapse text-left">
              <thead>
                <tr className="border-b border-(--color-border-strong)">
                  <th className="px-6 py-3 font-mono text-xs uppercase tracking-wider text-(--color-faint) font-normal">
                    Format
                  </th>
                  <th className="px-0 pr-6 py-3 font-mono text-xs uppercase tracking-wider text-(--color-faint) font-normal text-center">
                    Decode
                  </th>
                  <th className="px-0 pr-6 py-3 font-mono text-xs uppercase tracking-wider text-(--color-faint) font-normal text-center">
                    Encode
                  </th>
                  <th className="py-3 font-mono text-xs uppercase tracking-wider text-(--color-faint) font-normal">
                    Notes
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-(--color-border)">
                {formatRows.map((row: FormatRow) => (
                  <tr key={row.format} className="group hover:bg-(--color-surface-2) transition-colors duration-100">
                    <td className="px-6 py-3 font-mono text-sm text-(--color-fg) whitespace-nowrap">
                      {row.format}
                      {row.decode === 'yes' && row.encode === 'yes' && (
                        <span className="ml-2 inline-block rounded-full bg-(--color-accent-muted) px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider text-(--color-accent) leading-none align-middle">
                          rw
                        </span>
                      )}
                    </td>
                    <td className="pr-6 py-3 text-center">
                      <SupportCell s={row.decode} />
                    </td>
                    <td className="pr-6 py-3 text-center">
                      <SupportCell s={row.encode} />
                    </td>
                    <td className="py-3 text-sm text-(--color-muted)">
                      {row.note ?? null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className="mt-4 font-mono text-xs text-(--color-faint) tabular-nums">
            {formatRows.length} formats total · {formatRows.filter(r => r.decode === 'yes' && r.encode === 'yes').length} bidirectional · {formatRows.filter(r => r.encode === 'no').length} decode-only
          </p>
        </Reveal>
      </div>
    </section>
  )
}
