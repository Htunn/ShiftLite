import { Link, useLocation } from 'react-router-dom';

/** Top navigation bar (SPEC-001 UI Architecture). */
export default function Navbar() {
  const location = useLocation();
  return (
    <header className="flex items-center justify-between border-b border-slate-800 bg-slate-900 px-6 py-3">
      <Link to="/" className="text-lg font-semibold text-slate-100">
        KubeWeave
      </Link>
      <nav className="flex gap-4 text-sm text-slate-400">
        <Link className={location.pathname === '/' ? 'text-slate-100' : undefined} to="/">
          Overview
        </Link>
      </nav>
    </header>
  );
}
