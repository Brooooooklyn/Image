import { formatRows, matrixCaption } from '../_data/formats'

const Cell = ({ s, note }: { s: 'yes' | 'no'; note?: string }) =>
  s === 'yes'
    ? <span className="text-(--color-accent)">✓{note ? ` (${note})` : ''}</span>
    : <span className="text-(--color-muted)">—{note ? ` (${note})` : ''}</span>

export default function FormatMatrix() {
  return (
    <section className="mx-auto max-w-4xl px-6 py-20">
      <h2 className="text-3xl font-bold">Formats</h2>
      <p className="mt-2 text-(--color-muted)">{matrixCaption}</p>
      <table className="mt-8 w-full border-collapse text-sm">
        <thead><tr className="border-b border-white/10 text-left text-(--color-muted)">
          <th className="py-2">Format</th><th className="py-2">Decode</th><th className="py-2">Encode</th></tr></thead>
        <tbody className="font-mono">{formatRows.map((r) => (
          <tr key={r.format} className="border-b border-white/5">
            <td className="py-2">{r.format}</td>
            <td className="py-2"><Cell s={r.decode} /></td>
            <td className="py-2"><Cell s={r.encode} note={r.encode === 'no' ? r.note : undefined} /></td>
          </tr>))}</tbody>
      </table>
    </section>
  )
}
