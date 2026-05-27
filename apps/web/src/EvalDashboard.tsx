import { useState, useEffect } from 'react';
import { ClipboardCheck } from 'lucide-react';
import { StatusBadge } from './IconSystem';

export interface EvalSummary {
  trace_id: string;
  span_id: string;
  name: string;
  start_time: number;
  status: string;
  attributes: Record<string, unknown>;
}

export function EvalDashboard({ onSelectTrace }: { onSelectTrace: (id: string) => void }) {
  const [evals, setEvals] = useState<EvalSummary[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetch('http://127.0.0.1:3000/api/evals')
      .then((res) => res.json())
      .then((data: EvalSummary[]) => {
        setEvals(data);
        setLoading(false);
      })
      .catch((err: unknown) => {
        console.error('Failed to fetch evals', err);
        setLoading(false);
      });
  }, []);

  if (loading) return <div className="p-8 text-slate-500">Loading evaluations...</div>;
  if (evals.length === 0)
    return (
      <div className="p-8 text-slate-500">No evaluations found. Run a local eval runner first.</div>
    );

  return (
    <div className="mx-auto max-w-7xl p-6">
      <div className="mb-5 flex items-center gap-3">
        <div className="rounded border border-slate-200 bg-white p-2 text-slate-700 shadow-sm">
          <ClipboardCheck className="h-5 w-5" aria-hidden="true" />
        </div>
        <h1 className="text-xl font-bold text-slate-900">Local Evaluations</h1>
      </div>
      <div className="overflow-hidden rounded border border-slate-200 bg-white shadow-sm">
        <table className="w-full text-left border-collapse">
          <thead>
            <tr className="bg-slate-50 border-b border-slate-200 text-sm uppercase text-slate-500">
              <th className="px-6 py-3 font-medium">Dataset/Name</th>
              <th className="px-6 py-3 font-medium">Result</th>
              <th className="px-6 py-3 font-medium">Trace ID</th>
              <th className="px-6 py-3 font-medium">Time</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {evals.map((e) => {
              const passed = e.attributes['eval.passed'] as boolean | undefined;
              const isPassed = passed === true;
              return (
                <tr key={e.span_id} className="hover:bg-slate-50 transition-colors">
                  <td className="px-6 py-4 font-medium text-slate-800">{e.name}</td>
                  <td className="px-6 py-4">
                    <StatusBadge status={isPassed ? 'pass' : 'fail'} />
                  </td>
                  <td
                    className="max-w-[240px] cursor-pointer truncate px-6 py-4 font-mono text-sm text-sky-700 hover:underline"
                    onClick={() => onSelectTrace(e.trace_id)}
                  >
                    {e.trace_id}
                  </td>
                  <td className="px-6 py-4 text-slate-500 text-sm">
                    {new Date(e.start_time).toLocaleString()}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
