import { useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Database, GitCompareArrows, Search, Workflow } from 'lucide-react';
import { api, queryKeys } from './api';
import { MetricPill, SpanKindBadge, StatusBadge } from './IconSystem';

export function TraceList({
  onSelectTrace,
  onDiffTraces,
}: {
  onSelectTrace: (id: string) => void;
  onDiffTraces: (idA: string, idB: string) => void;
}) {
  const [selectedForDiff, setSelectedForDiff] = useState<string[]>([]);
  const [query, setQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<'all' | 'ok' | 'error' | 'running'>('all');
  const parentRef = useRef<HTMLDivElement | null>(null);

  const {
    data: traces = [],
    isLoading,
    error,
  } = useQuery({
    queryKey: queryKeys.traces,
    queryFn: api.listTraces,
  });

  const handleCheckbox = (traceId: string) => {
    setSelectedForDiff((prev) => {
      if (prev.includes(traceId)) return prev.filter((id) => id !== traceId);
      if (prev.length >= 2) return [prev[1], traceId]; // Keep only 2
      return [...prev, traceId];
    });
  };

  const filteredTraces = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return traces.filter((trace) => {
      const statusBucket = trace.end_time ? trace.status.toLowerCase() : 'running';
      if (statusFilter !== 'all' && statusBucket !== statusFilter) return false;
      if (!normalizedQuery) return true;
      return [
        trace.trace_id,
        trace.run_id,
        trace.root_name,
        trace.root_span_kind,
        trace.model_provider,
        trace.model_name,
        trace.error_summary,
        trace.status,
      ]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(normalizedQuery));
    });
  }, [query, statusFilter, traces]);

  const rowVirtualizer = useVirtualizer({
    count: filteredTraces.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 84,
    overscan: 8,
  });

  if (isLoading) return <div className="p-8 text-ink-dim">Loading traces...</div>;
  if (error) return <div className="p-8 text-error">{error.message}</div>;
  if (traces.length === 0)
    return <div className="p-8 text-ink-dim">No traces found. Run a local agent first.</div>;

  const GRID_COLS = '40px minmax(240px,1.4fr) minmax(180px,1fr) 120px 70px 110px 150px';
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
        <div className="flex flex-col items-stretch gap-3 md:items-end">
          <div className="flex flex-wrap items-center gap-2">
            <label className="relative">
              <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-dim" />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search traces"
                className="h-9 w-[260px] rounded-pill border border-line-input bg-panel pl-9 pr-3 font-mono text-xs text-ink-hi outline-none transition-colors placeholder:text-ink-faint focus:border-iris"
              />
            </label>
            <select
              value={statusFilter}
              onChange={(event) =>
                setStatusFilter(event.target.value as 'all' | 'ok' | 'error' | 'running')
              }
              className="h-9 rounded-pill border border-line-input bg-panel px-3 text-xs font-semibold text-ink-mid outline-none focus:border-iris"
            >
              <option value="all">All status</option>
              <option value="ok">OK</option>
              <option value="error">Error</option>
              <option value="running">Running</option>
            </select>
          </div>
          {selectedForDiff.length === 2 && (
            <button
              onClick={() => onDiffTraces(selectedForDiff[0], selectedForDiff[1])}
              className="inline-flex items-center justify-center gap-2 rounded-pill bg-iris px-4 py-2 text-sm font-semibold text-window shadow-iris transition-colors"
            >
              <GitCompareArrows className="h-4 w-4" aria-hidden="true" />
              Compare selected · 2
            </button>
          )}
        </div>
      </div>

      <div className="overflow-hidden rounded-panel border border-line-inner bg-surface">
        <div className="grid bg-panel" style={{ gridTemplateColumns: GRID_COLS }}>
          <div className="label-th px-4 py-3" />
          <div className="label-th px-4 py-3">Run</div>
          <div className="label-th px-4 py-3">Model</div>
          <div className="label-th px-4 py-3">Status</div>
          <div className="label-th px-4 py-3">Spans</div>
          <div className="label-th px-4 py-3">Duration</div>
          <div className="label-th px-4 py-3">Time</div>
        </div>

        <div ref={parentRef} className="max-h-[calc(100vh-260px)] overflow-auto">
          <div className="relative" style={{ height: `${rowVirtualizer.getTotalSize()}px` }}>
            {rowVirtualizer.getVirtualItems().map((virtualRow) => {
              const trace = filteredTraces[virtualRow.index];
              const selected = selectedForDiff.includes(trace.trace_id);
              const displayStatus = trace.end_time ? trace.status : 'running';
              const model =
                trace.model_provider || trace.model_name
                  ? [trace.model_provider, trace.model_name].filter(Boolean).join(' / ')
                  : '—';
              return (
                <div
                  key={trace.trace_id}
                  className={`absolute left-0 top-0 grid w-full items-center border-b border-line-row transition-colors duration-[0.15s] hover:bg-[rgba(124,131,255,0.05)] ${
                    selected ? 'bg-[rgba(124,131,255,0.07)]' : ''
                  }`}
                  style={{
                    gridTemplateColumns: GRID_COLS,
                    height: `${virtualRow.size}px`,
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
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
                    className="min-w-0 cursor-pointer px-4 py-4"
                    onClick={() => onSelectTrace(trace.trace_id)}
                  >
                    <div className="mb-1 flex min-w-0 items-center gap-2">
                      {trace.root_span_kind && <SpanKindBadge kind={trace.root_span_kind} />}
                      <span className="truncate text-sm font-semibold text-ink-hi">
                        {trace.root_name ?? 'Unnamed run'}
                      </span>
                    </div>
                    <div className="truncate font-mono text-[11px] text-iris-text">
                      {trace.trace_id}
                    </div>
                    {trace.error_summary && (
                      <div className="mt-1 truncate text-xs text-error">{trace.error_summary}</div>
                    )}
                  </div>
                  <div
                    className="cursor-pointer truncate px-4 py-4 font-mono text-xs text-ink-mid"
                    onClick={() => onSelectTrace(trace.trace_id)}
                  >
                    {model}
                  </div>
                  <div
                    className="cursor-pointer px-4 py-4"
                    onClick={() => onSelectTrace(trace.trace_id)}
                  >
                    <StatusBadge status={displayStatus} />
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
        {filteredTraces.length === 0 && (
          <div className="p-8 text-center text-ink-dim">No traces match the current filter.</div>
        )}
      </div>
    </div>
  );
}
