import { useQuery } from '@tanstack/react-query';
import StatusBadge from '../components/common/StatusBadge';

interface ClusterInfo {
  id: string;
  name: string;
  type: 'k8s' | 'k3s';
  version: string;
  status: 'Ready' | 'Unreachable';
}

async function fetchClusters(): Promise<ClusterInfo[]> {
  const res = await fetch('/api/v1/clusters', { credentials: 'include' });
  if (!res.ok) throw new Error(`clusters request failed: ${res.status}`);
  return res.json();
}

/** Cluster engine summary, k8s vs k3s (SPEC-001 UI Architecture). */
export default function Overview() {
  const { data: clusters, isLoading, error } = useQuery({ queryKey: ['clusters'], queryFn: fetchClusters });

  if (isLoading) return <p className="text-slate-400">Loading clusters…</p>;
  if (error) return <p className="text-red-400">Failed to load clusters.</p>;

  return (
    <div className="space-y-4">
      <h1 className="text-lg font-semibold">Clusters</h1>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {clusters?.map((c) => (
          <div key={c.id} className="rounded border border-slate-800 bg-slate-900 p-4">
            <div className="flex items-center justify-between">
              <span className="font-medium">{c.name}</span>
              <StatusBadge status={c.status === 'Ready' ? 'Running' : 'Failed'} />
            </div>
            <p className="mt-1 text-sm text-slate-400">
              {c.type} · v{c.version}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}
