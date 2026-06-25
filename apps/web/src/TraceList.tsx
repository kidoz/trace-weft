import { useState, useEffect } from 'react';
import { Database, GitCompareArrows, Workflow } from 'lucide-react';
import { apiUrl } from './api';
import { MetricPill, StatusBadge } from './IconSystem';

export interface TraceSummary {
  trace_id: string;
  run_id: string;
  start_time: number;
  end_time: number | null;
  span_count: number;
  status: string;
}

export function TraceList({
  onSelectTrace,
  onDiffTraces,
}: {
  onSelectTrace: (id: string) => void;
  onDiffTraces: (idA: string, idB: string) => void;
}) {
  const [traces, setTraces] = useState<TraceSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedForDiff, setSelectedForDiff] = useState<string[]>([]);

  useEffect(() => {
    fetch(apiUrl('/api/traces'))
      .then((res) => res.json())
      .then((data) => {
        setTraces(data);
        setLoading(false);
      })
      .catch((err) => {
        console.error('Failed to fetch traces', err);
        setLoading(false);
      });
  }, []);

  const handleCheckbox = (traceId: string) => {
    setSelectedForDiff((prev) => {
      if (prev.includes(traceId)) return prev.filter((id) => id !== traceId);
      if (prev.length >= 2) return [prev[1], traceId]; // Keep only 2
      return [...prev, traceId];
    });
  };

  if (loading) return <div className="p-8 text-ink-dim">Loading traces...</div>;
  if (traces.length === 0)
    return <div className="p-8 text-ink-dim">No traces found. Run a local agent first.</div>;

  const GRID_COLS = '40px 1fr 120px 70px 110px 150px';
  const totalSpans = traces.reduce((total, trace) => total + trace.span_count, 0);

  return (
    <div className="mx-auto max-w-7xl p-6">
      <div className="mb-5 flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
        <div>
          <h1 className="text-[24px] font-bold tracking-[-0.02em] text-ink-hi">Runs</h1>
          <div className="mt-3 flex flex-wrap gap-2">
            <MetricPill icon={Database} label={`${traces.length} local traces`} />
            <MetricPill icon={Workflow} label={`${totalSpans} spans`} />
          </div>
        </div>
        {selectedForDiff.length === 2 && (
          <button
            onClick={() => onDiffTraces(selectedForDiff[0], selectedForDiff[1])}
            className="inline-flex items-center gap-2 rounded-pill bg-iris px-4 py-2 text-sm font-semibold text-window shadow-iris transition-colors"
          >
            <GitCompareArrows className="h-4 w-4" aria-hidden="true" />
            Compare selected · 2
          </button>
        )}
      </div>

      <div className="overflow-hidden rounded-panel border border-line-inner bg-surface">
        <div className="grid bg-panel" style={{ gridTemplateColumns: GRID_COLS }}>
          <div className="label-th px-4 py-3" />
          <div className="label-th px-4 py-3">Trace ID</div>
          <div className="label-th px-4 py-3">Status</div>
          <div className="label-th px-4 py-3">Spans</div>
          <div className="label-th px-4 py-3">Duration</div>
          <div className="label-th px-4 py-3">Time</div>
        </div>

        {traces.map((trace) => {
          const selected = selectedForDiff.includes(trace.trace_id);
          return (
            <div
              key={trace.trace_id}
              className={`grid items-center border-b border-line-row transition-colors duration-[0.15s] hover:bg-[rgba(124,131,255,0.05)] ${
                selected ? 'bg-[rgba(124,131,255,0.07)]' : ''
              }`}
              style={{ gridTemplateColumns: GRID_COLS }}
            >
              <div className="flex items-center justify-center px-4 py-4">
                <input
                  type="checkbox"
                  checked={selected}
                  onChange={() => handleCheckbox(trace.trace_id)}
                  className="cursor-pointer accent-[#7c83ff]"
                />
              </div>
              <div
                className="cursor-pointer truncate px-4 py-4 font-mono text-sm text-iris-text"
                onClick={() => onSelectTrace(trace.trace_id)}
              >
                {trace.trace_id}
              </div>
              <div
                className="cursor-pointer px-4 py-4"
                onClick={() => onSelectTrace(trace.trace_id)}
              >
                <StatusBadge status={trace.status} />
              </div>
              <div
                className="cursor-pointer px-4 py-4 text-sm text-ink-mid"
                onClick={() => onSelectTrace(trace.trace_id)}
              >
                {trace.span_count}
              </div>
              <div
                className="cursor-pointer px-4 py-4 font-mono text-sm text-ink-hi"
                onClick={() => onSelectTrace(trace.trace_id)}
              >
                {trace.end_time ? `${trace.end_time - trace.start_time}ms` : 'Running'}
              </div>
              <div
                className="cursor-pointer px-4 py-4 text-sm text-ink-dim"
                onClick={() => onSelectTrace(trace.trace_id)}
              >
                {new Date(trace.start_time).toLocaleString()}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
