/**
 * Service & pod topology graph (SPEC-001 UI Architecture). Placeholder pending a graph
 * rendering library decision (e.g. react-flow); renders a simple node list for now.
 */
export default function GraphView({ nodes }: { nodes: string[] }) {
  if (nodes.length === 0) {
    return <p className="text-sm text-slate-500">No topology data yet.</p>;
  }
  return (
    <ul className="flex flex-wrap gap-2">
      {nodes.map((n) => (
        <li key={n} className="rounded border border-slate-700 px-3 py-1 text-sm">
          {n}
        </li>
      ))}
    </ul>
  );
}
