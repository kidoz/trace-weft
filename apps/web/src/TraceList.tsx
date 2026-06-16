import { useState, useEffect } from 'react';
import { Database, GitCompareArrows, Timer, Workflow } from 'lucide-react';
import { apiUrl } from './api';
import { navigationIcons } from './IconRegistry';
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

  if (loading) return <div className="p-8 text-slate-500">Loading traces...</div>;
  if (traces.length === 0)
    return <div className="p-8 text-slate-500">No traces found. Run a local agent first.</div>;

  const RunsIcon = navigationIcons.runs;

  return (
    <div className="mx-auto max-w-7xl p-6">
      <div className="mb-5 flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
        <div className="flex items-start gap-3">
          <div className="rounded border border-slate-200 bg-white p-2 text-slate-700 shadow-sm">
            <RunsIcon className="h-5 w-5" aria-hidden="true" />
          </div>
          <div>
            <h1 className="text-xl font-bold text-slate-900">Runs</h1>
            <div className="mt-2 flex flex-wrap gap-2">
              <MetricPill icon={Database} label={`${traces.length} local traces`} />
              <MetricPill
                icon={Workflow}
                label={`${traces.reduce((total, trace) => total + trace.span_count, 0)} spans`}
              />
            </div>
          </div>
        </div>
        {selectedForDiff.length === 2 && (
          <button
            onClick={() => onDiffTraces(selectedForDiff[0], selectedForDiff[1])}
            className="inline-flex items-center gap-2 rounded bg-slate-950 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-slate-800"
          >
            <GitCompareArrows className="h-4 w-4" aria-hidden="true" />
            Compare Selected ({selectedForDiff.length})
          </button>
        )}
      </div>
      <div className="overflow-hidden rounded border border-slate-200 bg-white shadow-sm">
        <table className="w-full text-left border-collapse">
          <thead>
            <tr className="bg-slate-50 border-b border-slate-200 text-sm uppercase text-slate-500">
              <th className="px-6 py-3 font-medium w-10">Diff</th>
              <th className="px-6 py-3 font-medium">Trace ID</th>
              <th className="px-6 py-3 font-medium">Status</th>
              <th className="px-6 py-3 font-medium">Spans</th>
              <th className="px-6 py-3 font-medium">Duration</th>
              <th className="px-6 py-3 font-medium">Time</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {traces.map((trace) => (
              <tr
                key={trace.trace_id}
                className={`hover:bg-slate-50 transition-colors ${selectedForDiff.includes(trace.trace_id) ? 'bg-blue-50/50' : ''}`}
              >
                <td className="px-6 py-4">
                  <input
                    type="checkbox"
                    checked={selectedForDiff.includes(trace.trace_id)}
                    onChange={() => handleCheckbox(trace.trace_id)}
                    className="cursor-pointer"
                  />
                </td>
                <td
                  className="max-w-[240px] cursor-pointer truncate px-6 py-4 font-mono text-sm text-sky-700"
                  onClick={() => onSelectTrace(trace.trace_id)}
                >
                  {trace.trace_id}
                </td>
                <td
                  className="px-6 py-4 cursor-pointer"
                  onClick={() => onSelectTrace(trace.trace_id)}
                >
                  <StatusBadge status={trace.status} />
                </td>
                <td
                  className="px-6 py-4 text-slate-600 text-sm cursor-pointer"
                  onClick={() => onSelectTrace(trace.trace_id)}
                >
                  {trace.span_count}
                </td>
                <td
                  className="px-6 py-4 text-slate-600 text-sm cursor-pointer"
                  onClick={() => onSelectTrace(trace.trace_id)}
                >
                  <span className="inline-flex items-center gap-1.5">
                    <Timer className="h-3.5 w-3.5 text-slate-400" aria-hidden="true" />
                    {trace.end_time ? `${trace.end_time - trace.start_time}ms` : 'Running'}
                  </span>
                </td>
                <td
                  className="px-6 py-4 text-slate-500 text-sm cursor-pointer"
                  onClick={() => onSelectTrace(trace.trace_id)}
                >
                  {new Date(trace.start_time).toLocaleString()}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
