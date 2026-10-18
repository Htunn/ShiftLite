import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';

interface ClusterOption {
  id: string;
  name: string;
  allowedBuilders: string[];
}

interface CreateAppPayload {
  name: string;
  gitRepository: string;
  branch: string;
  targetCluster: string;
  builder: string;
  replicas: number;
}

async function createApp(namespace: string, payload: CreateAppPayload) {
  const res = await fetch(`/api/v1/namespaces/${namespace}/apps`, {
    method: 'POST',
    credentials: 'include',
    headers: { 'Content-Type': 'application/json', 'X-Requested-With': 'shiftlite' },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ code: 'internal', message: res.statusText }));
    throw new Error(body.code ?? 'internal');
  }
  return res.json();
}

/** Source-to-Deployment creation wizard (SPEC-001 UI Architecture). */
export default function CreateAppWizard({ namespace, clusters }: { namespace: string; clusters: ClusterOption[] }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState('');
  const [gitRepository, setGitRepository] = useState('');
  const [branch, setBranch] = useState('main');
  const [targetCluster, setTargetCluster] = useState(clusters[0]?.id ?? '');
  const [builder, setBuilder] = useState('buildkit');
  const [replicas, setReplicas] = useState(1);
  const [error, setError] = useState<string | null>(null);

  const selectedCluster = clusters.find((c) => c.id === targetCluster);
  const allowedBuilders = selectedCluster?.allowedBuilders ?? ['buildkit'];

  const mutation = useMutation({
    mutationFn: () => createApp(namespace, { name, gitRepository, branch, targetCluster, builder, replicas }),
    onSuccess: () => {
      setError(null);
      queryClient.invalidateQueries({ queryKey: ['apps', namespace] });
    },
    onError: (e: Error) => setError(e.message),
  });

  return (
    <form
      className="space-y-3 rounded border border-slate-800 bg-slate-900 p-4"
      onSubmit={(e) => {
        e.preventDefault();
        mutation.mutate();
      }}
    >
      <h2 className="text-sm font-semibold text-slate-200">Deploy a new application</h2>

      {error && <p className="rounded bg-red-500/10 px-3 py-2 text-sm text-red-300">{error}</p>}

      <label className="block text-sm">
        Name
        <input
          className="mt-1 w-full rounded bg-slate-800 px-2 py-1"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
        />
      </label>

      <label className="block text-sm">
        Git repository (https://)
        <input
          className="mt-1 w-full rounded bg-slate-800 px-2 py-1"
          value={gitRepository}
          onChange={(e) => setGitRepository(e.target.value)}
          placeholder="https://github.com/org/app.git"
          required
        />
      </label>

      <label className="block text-sm">
        Branch
        <input
          className="mt-1 w-full rounded bg-slate-800 px-2 py-1"
          value={branch}
          onChange={(e) => setBranch(e.target.value)}
        />
      </label>

      <label className="block text-sm">
        Target cluster
        <select
          className="mt-1 w-full rounded bg-slate-800 px-2 py-1"
          value={targetCluster}
          onChange={(e) => setTargetCluster(e.target.value)}
        >
          {clusters.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
      </label>

      <label className="block text-sm">
        Builder
        <select
          className="mt-1 w-full rounded bg-slate-800 px-2 py-1"
          value={builder}
          onChange={(e) => setBuilder(e.target.value)}
        >
          {allowedBuilders.map((b) => (
            <option key={b} value={b}>
              {b}
            </option>
          ))}
        </select>
      </label>

      <label className="block text-sm">
        Replicas
        <input
          type="number"
          min={0}
          max={50}
          className="mt-1 w-full rounded bg-slate-800 px-2 py-1"
          value={replicas}
          onChange={(e) => setReplicas(Number(e.target.value))}
        />
      </label>

      <button
        type="submit"
        disabled={mutation.isPending}
        className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
      >
        {mutation.isPending ? 'Deploying…' : 'Deploy'}
      </button>
    </form>
  );
}
