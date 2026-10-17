export type AppPhase = 'Pending' | 'Building' | 'Deploying' | 'Running' | 'Degraded' | 'Failed';

const COLORS: Record<AppPhase, string> = {
  Pending: 'bg-yellow-500/20 text-yellow-300',
  Building: 'bg-yellow-500/20 text-yellow-300',
  Deploying: 'bg-yellow-500/20 text-yellow-300',
  Running: 'bg-green-500/20 text-green-300',
  Degraded: 'bg-orange-500/20 text-orange-300',
  Failed: 'bg-red-500/20 text-red-300',
};

/** Status indicator colors per SPEC-001: Running=green, Building/Deploying/Pending=yellow,
 * Degraded=orange, Failed=red. */
export default function StatusBadge({ status }: { status: AppPhase }) {
  return (
    <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${COLORS[status]}`}>{status}</span>
  );
}
