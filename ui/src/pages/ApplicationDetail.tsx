import { useParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import StatusBadge, { type AppPhase } from '../components/common/StatusBadge';
import TerminalStream from '../components/logs/TerminalStream';
import GraphView from '../components/topology/GraphView';

interface AppDetail {
  name: string;
  namespace: string;
  targetCluster: string;
  status: AppPhase;
  routeUrl?: string;
  gitRepository: string;
  branch: string;
  builder: string;
  replicas: number;
  availableReplicas: number;
  message?: string;
}

async function fetchApp(namespace: string, appName: string): Promise<AppDetail> {
  const res = await fetch(`/api/v1/namespaces/${namespace}/apps/${appName}`, { credentials: 'include' });
  if (!res.ok) throw new Error(`app request failed: ${res.status}`);
  return res.json();
}

/** Application detail page (SPEC-001 UI Architecture): status, topology, and live logs. */
export default function ApplicationDetail() {
  const { namespace = '', appName = '' } = useParams();
  const { data: app, isLoading, error } = useQuery({
    queryKey: ['apps', namespace, appName],
    queryFn: () => fetchApp(namespace, appName),
    enabled: Boolean(namespace && appName),
  });

  if (isLoading) return <p className="text-slate-400">Loading…</p>;
  if (error || !app) return <p className="text-red-400">Application not found.</p>;

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <h1 className="text-lg font-semibold">{app.name}</h1>
        <StatusBadge status={app.status} />
      </div>
      <dl className="grid grid-cols-2 gap-2 text-sm text-slate-400 sm:grid-cols-4">
        <div>
          <dt>Cluster</dt>
          <dd className="text-slate-200">{app.targetCluster}</dd>
        </div>
        <div>
          <dt>Branch</dt>
          <dd className="text-slate-200">{app.branch}</dd>
        </div>
        <div>
          <dt>Builder</dt>
          <dd className="text-slate-200">{app.builder}</dd>
        </div>
        <div>
          <dt>Replicas</dt>
          <dd className="text-slate-200">
            {app.availableReplicas}/{app.replicas}
          </dd>
        </div>
      </dl>
      {app.message && <p className="rounded bg-red-500/10 px-3 py-2 text-sm text-red-300">{app.message}</p>}

      <section>
        <h2 className="mb-2 text-sm font-semibold text-slate-300">Topology</h2>
        <GraphView nodes={[app.name]} />
      </section>

      <section>
        <h2 className="mb-2 text-sm font-semibold text-slate-300">Logs</h2>
        <TerminalStream namespace={namespace} appName={appName} cluster={app.targetCluster} />
      </section>
    </div>
  );
}
