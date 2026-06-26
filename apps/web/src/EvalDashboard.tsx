import { useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useVirtualizer } from '@tanstack/react-virtual';
import { api, queryKeys } from './api';
import { StatusBadge } from './IconSystem';

export function EvalDashboard({ onSelectTrace }: { onSelectTrace: (id: string) => void }) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const {
    data: evals = [],
    isLoading,
    error,
  } = useQuery({
    queryKey: queryKeys.evals,
    queryFn: api.listEvals,
  });
  const virtualizer = useVirtualizer({
    count: evals.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 54,
    overscan: 10,
  });

  if (isLoading) return <div className="p-8 text-ink-dim">Loading evaluations...</div>;
  if (error) return <div className="p-8 text-error">{error.message}</div>;
  if (evals.length === 0)
    return (
      <div className="p-8 text-ink-dim">No evaluations found. Run a local eval runner first.</div>
    );

  const total = evals.length;
  const passed = evals.filter((e) => e.attributes['eval.passed'] === true).length;
  const passPct = total > 0 ? Math.round((passed / total) * 100) : 0;
  const datasetCount = new Set(evals.map((e) => e.name)).size;

  const scores = evals
    .map((e) => e.attributes['eval.score'])
    .filter((s): s is number => typeof s === 'number');
  const avgScore =
    scores.length > 0 ? (scores.reduce((acc, s) => acc + s, 0) / scores.length).toFixed(2) : '—';

  return (
    <div className="mx-auto max-w-7xl p-6">
      <h1 className="mb-5 text-[24px] font-bold tracking-[-0.02em] text-ink-hi">
        Local Evaluations
      </h1>

      <div className="mb-5 grid grid-cols-3 gap-4">
        <div className="rounded-panel border border-line-inner bg-nav p-5">
          <div className="label-section text-ink-mid">Pass rate</div>
          <div className="mt-2 font-mono text-[28px] text-ok">{passPct}%</div>
          <div className="mt-1 font-mono text-xs text-ink-dim">
            {passed} / {total}
          </div>
          <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-[#1b1f27]">
            <div className="h-full rounded-full bg-ok" style={{ width: `${passPct}%` }} />
          </div>
        </div>

        <div className="rounded-panel border border-line-inner bg-nav p-5">
          <div className="label-section text-ink-mid">Evaluations</div>
          <div className="mt-2 font-mono text-[28px] text-ink-hi">{total}</div>
          <div className="mt-1 text-xs text-ink-dim">across {datasetCount} datasets</div>
        </div>

        <div className="rounded-panel border border-line-inner bg-nav p-5">
          <div className="label-section text-ink-mid">Avg score</div>
          <div className="mt-2 font-mono text-[28px] text-iris">{avgScore}</div>
          <div className="mt-1 text-xs text-ink-dim">mean score</div>
        </div>
      </div>

      <div className="overflow-hidden rounded-panel border border-line-inner bg-surface">
        <div className="grid grid-cols-[1fr_110px_90px_220px_130px] bg-panel">
          <div className="label-th px-4 py-3">Dataset/Name</div>
          <div className="label-th px-4 py-3">Result</div>
          <div className="label-th px-4 py-3">Score</div>
          <div className="label-th px-4 py-3">Trace ID</div>
          <div className="label-th px-4 py-3">Time</div>
        </div>
        <div ref={parentRef} className="max-h-[calc(100vh-330px)] overflow-auto">
          <div className="relative" style={{ height: `${virtualizer.getTotalSize()}px` }}>
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const e = evals[virtualRow.index];
              const isPassed = e.attributes['eval.passed'] === true;
              const rawScore = e.attributes['eval.score'];
              const score = typeof rawScore === 'number' ? rawScore.toFixed(2) : '—';
              return (
                <div
                  key={e.span_id}
                  className="absolute left-0 top-0 grid w-full grid-cols-[1fr_110px_90px_220px_130px] items-center border-b border-line-row transition-colors hover:bg-[rgba(124,131,255,0.05)]"
                  style={{
                    height: `${virtualRow.size}px`,
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                >
                  <div className="px-4 py-3 font-medium text-ink-hi">{e.name}</div>
                  <div className="px-4 py-3">
                    <StatusBadge status={isPassed ? 'pass' : 'fail'} />
                  </div>
                  <div
                    className={`px-4 py-3 font-mono text-sm ${isPassed ? 'text-ink-hi' : 'text-error'}`}
                  >
                    {score}
                  </div>
                  <div
                    className="cursor-pointer truncate px-4 py-3 font-mono text-sm text-iris-text hover:underline"
                    onClick={() => onSelectTrace(e.trace_id)}
                  >
                    {e.trace_id}
                  </div>
                  <div className="px-4 py-3 text-sm text-ink-dim">
                    {new Date(e.start_time).toLocaleString()}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
