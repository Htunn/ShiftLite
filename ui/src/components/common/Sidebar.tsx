/** Left-hand navigation (SPEC-001 UI Architecture). */
export default function Sidebar() {
  return (
    <aside className="w-56 shrink-0 border-r border-slate-800 bg-slate-900/50 p-4 text-sm text-slate-400">
      <ul className="space-y-2">
        <li>Clusters</li>
        <li>Applications</li>
      </ul>
    </aside>
  );
}
