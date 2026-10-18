const PROVIDERS: { id: string; label: string }[] = [
  { id: 'github', label: 'GitHub' },
  { id: 'entraid', label: 'Microsoft Entra ID' },
  { id: 'keycloak', label: 'Keycloak' },
  { id: 'rhsso', label: 'Red Hat SSO' },
];

/** Multi-provider SSO login page (SPEC-001 UI Architecture). */
export default function Login() {
  const params = new URLSearchParams(window.location.search);
  const error = params.get('error');

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="w-full max-w-sm space-y-4 rounded border border-slate-800 bg-slate-900 p-6">
        <h1 className="text-xl font-semibold">Sign in to ShiftLite</h1>
        {error && (
          <p className="rounded bg-red-500/10 px-3 py-2 text-sm text-red-300">
            Sign-in failed: {error}
          </p>
        )}
        <div className="space-y-2">
          {PROVIDERS.map((p) => (
            <a
              key={p.id}
              href={`/api/v1/auth/login?provider=${p.id}`}
              className="block rounded bg-slate-800 px-4 py-2 text-center text-sm hover:bg-slate-700"
            >
              Continue with {p.label}
            </a>
          ))}
        </div>
      </div>
    </div>
  );
}
