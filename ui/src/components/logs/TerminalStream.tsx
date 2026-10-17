import { useEffect, useRef, useState } from 'react';

const MAX_LINES = 10_000;

interface LogEvent {
  ts: string;
  pod: string;
  container: string;
  stream: 'stdout' | 'stderr';
  line: string;
}

/**
 * Real-time SSE log viewer (SPEC-001). Uses the native `EventSource`, which resumes with
 * `Last-Event-ID` automatically on reconnect, and caps its buffer at 10,000 lines.
 */
export default function TerminalStream({ namespace, appName, cluster }: { namespace: string; appName: string; cluster?: string }) {
  const [lines, setLines] = useState<string[]>([]);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const url = new URL(`/api/v1/namespaces/${namespace}/apps/${appName}/logs/stream`, window.location.origin);
    if (cluster) url.searchParams.set('cluster', cluster);

    const source = new EventSource(url.toString(), { withCredentials: true });

    source.addEventListener('log', (event) => {
      try {
        const data: LogEvent = JSON.parse((event as MessageEvent).data);
        setLines((prev) => {
          const next = [...prev, `${data.ts} ${data.line}`];
          return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
        });
      } catch {
        // ignore malformed events
      }
    });

    source.addEventListener('end', () => source.close());
    source.onerror = () => {
      // EventSource retries automatically; nothing to do here.
    };

    return () => source.close();
  }, [namespace, appName, cluster]);

  useEffect(() => {
    containerRef.current?.scrollTo({ top: containerRef.current.scrollHeight });
  }, [lines]);

  return (
    <div
      ref={containerRef}
      className="h-96 overflow-y-auto rounded bg-black p-3 font-mono text-xs text-green-400"
    >
      {lines.map((line, i) => (
        <div key={i}>{line}</div>
      ))}
    </div>
  );
}
