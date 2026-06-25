import { useState, useEffect, useMemo } from 'react';
import { ArrowLeft, Clock3, Network, Play, Rows3 } from 'lucide-react';
import { ReplayLab } from './ReplayLab';
import { TraceGraph } from './TraceGraph';
import { TokenHeatmap } from './TokenHeatmap';
import { MemoryDiff } from './MemoryDiff';
import { JsonView } from './JsonView';
import { SpanKindBadge } from './IconSystem';
import { spanKindColor } from './spanColors';
import { apiUrl } from './api';

export interface Span {
  trace_id: string;
  span_id: string;
  parent_span_id: string | null;
  span_kind: string;
  name: string;
  start_time: number;
  end_time: number | null;
  status: string;
  attributes: Record<string, unknown>;
  latency_ms: number | null;
  input_ref: unknown | null;
  output_ref: unknown | null;
  cost_estimate?: { amount: number; currency: string } | null;
  token_usage?: {
    input: number;
    output: number;
    reasoning: number | null;
    breakdown: Record<string, number>;
  } | null;
  memory_state?: Record<string, unknown> | null;
}

function isPending(status: string): boolean {
  const s = status.toLowerCase();
  return s === 'pending' || s === 'waiting' || s === 'pending_approval';
}

function depthOf(span: Span, byId: Map<string, Span>): number {
  let depth = 0;
  let current = span.parent_span_id;
  const seen = new Set<string>();
  while (current && byId.has(current) && !seen.has(current)) {
    seen.add(current);
    depth += 1;
    current = byId.get(current)!.parent_span_id;
  }
  return depth;
}

export function TraceDetail({ traceId, onBack }: { traceId: string; onBack: () => void }) {
  const [spans, setSpans] = useState<Span[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedSpanForReplay, setSelectedSpanForReplay] = useState<Span | null>(null);
  const [viewMode, setViewMode] = useState<'tree' | 'graph'>('tree');
  const [selectedSpan, setSelectedSpan] = useState<Span | null>(null);

  useEffect(() => {
    fetch(apiUrl(`/api/traces/${traceId}`))
      .then((res) => res.json())
      .then((data: Span[]) => {
        setSpans(data);
        if (data.length > 0) setSelectedSpan(data[0]);
        setLoading(false);
      })
      .catch((err: unknown) => {
        console.error('Failed to fetch trace', err);
        setLoading(false);
      });
  }, [traceId]);

  const { byId, window } = useMemo(() => {
    const map = new Map<string, Span>();
    spans.forEach((s) => map.set(s.span_id, s));
    let min = Infinity;
    let max = -Infinity;
    spans.forEach((s) => {
      min = Math.min(min, s.start_time);
      max = Math.max(max, s.end_time ?? s.start_time);
    });
    const span = max > min ? max - min : 1;
    return { byId: map, window: { min, total: span } };
  }, [spans]);

  if (loading) return <div className="p-8 text-ink-dim">Loading spans…</div>;

  if (selectedSpanForReplay) {
    return <ReplayLab span={selectedSpanForReplay} onBack={() => setSelectedSpanForReplay(null)} />;
  }

  const viewToggle = (mode: 'tree' | 'graph', label: string, Icon: typeof Rows3) => (
    <button
      onClick={() => setViewMode(mode)}
      className={`inline-flex items-center gap-2 rounded-pill px-3 py-1.5 text-[13px] font-semibold transition-colors ${
        viewMode === mode ? 'bg-iris text-window shadow-iris' : 'text-ink-mid hover:text-ink-hi'
      }`}
    >
      <Icon className="h-4 w-4" aria-hidden="true" />
      {label}
    </button>
  );

  const cost = selectedSpan?.cost_estimate?.amount;
  const tokens = selectedSpan?.token_usage;

  return (
    <div className="mx-auto flex h-full max-w-7xl flex-col p-6">
      {/* Sub-header */}
      <div className="mb-5 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <button
            onClick={onBack}
            className="inline-flex items-center gap-2 rounded-pill border border-line bg-panel px-3 py-2 text-[13px] font-medium text-ink-mid transition-colors hover:text-ink-hi"
          >
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
            Back
          </button>
          <div>
            <h1 className="text-[24px] font-bold tracking-[-0.02em] text-ink-hi">Trace Detail</h1>
            <div className="mt-1 flex flex-wrap items-center gap-2">
              <span className="font-mono text-xs text-ink-dim">{traceId}</span>
              <span className="inline-flex items-center gap-1 rounded-chip border border-[rgba(124,131,255,0.3)] bg-[rgba(124,131,255,0.12)] px-1.5 py-0.5 font-mono text-[11px] font-semibold text-iris-text">
                ⌗ {spans.length} spans
              </span>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-1 rounded-[9px] border border-line bg-panel p-1">
          {viewToggle('tree', 'Tree', Rows3)}
          {viewToggle('graph', 'Graph', Network)}
        </div>
      </div>

      {/* Body: tree/graph + inspector */}
      <div className="flex min-h-0 flex-1 overflow-hidden rounded-panel border border-line bg-surface shadow-window">
        {/* Left pane */}
        <div className="relative flex min-w-0 flex-1 flex-col border-r border-line-inner">
          {viewMode === 'tree' ? (
            <div className="flex-1 overflow-y-auto bg-surface p-4">
              <div className="label-section mb-3">Span Tree</div>
              <div className="flex flex-col gap-2">
                {spans.map((span) => {
                  const selected = selectedSpan?.span_id === span.span_id;
                  const pending = isPending(span.status);
                  const isRoot = !span.parent_span_id;
                  const depth = depthOf(span, byId);
                  const color = spanKindColor(span.span_kind);
                  const dur =
                    span.latency_ms ?? (span.end_time ? span.end_time - span.start_time : 0);
                  const left = isRoot ? 0 : ((span.start_time - window.min) / window.total) * 100;
                  const width = isRoot ? 100 : Math.max(2, (dur / window.total) * 100);
                  return (
                    <div
                      key={span.span_id}
                      onClick={() => setSelectedSpan(span)}
                      className="grid cursor-pointer grid-cols-[210px_1fr_80px] items-center gap-3 rounded-panel border p-3 transition-colors"
                      style={{
                        backgroundColor: selected ? 'rgba(124,131,255,0.08)' : '#13161b',
                        borderColor: selected
                          ? 'rgba(124,131,255,0.35)'
                          : pending
                            ? 'rgba(251,191,36,0.5)'
                            : '#20242c',
                        borderStyle: pending ? 'dashed' : 'solid',
                      }}
                    >
                      <div
                        className="flex min-w-0 items-center gap-2"
                        style={{ paddingLeft: depth * 30 }}
                      >
                        <SpanKindBadge kind={span.span_kind} />
                        <span
                          className={`truncate text-[13px] ${
                            isRoot || selected ? 'text-ink-hi' : 'text-ink-mid'
                          }`}
                        >
                          {span.name}
                        </span>
                      </div>

                      {pending ? (
                        <div className="inline-flex items-center gap-1.5 text-[11px] font-medium text-warn">
                          <Clock3 className="h-3.5 w-3.5" aria-hidden="true" />
                          awaiting approval
                        </div>
                      ) : (
                        <div className="relative h-2.5 w-full rounded-[5px] bg-[#1b1f27]">
                          <div
                            className="absolute top-0 h-full rounded-[5px]"
                            style={{
                              left: `${left}%`,
                              width: `${width}%`,
                              background: isRoot ? 'linear-gradient(90deg,#7c83ff,#9b8bff)' : color,
                            }}
                          />
                        </div>
                      )}

                      <div className="text-right font-mono text-[12px] text-ink-mid">
                        {dur ? `${dur}ms` : '—'}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          ) : (
            <div className="relative flex-1">
              <TraceGraph spans={spans} onSpanClick={setSelectedSpan} />
            </div>
          )}
        </div>

        {/* Right pane: Inspector */}
        <div className="flex w-[460px] shrink-0 flex-col overflow-y-auto bg-nav p-5">
          <div className="mb-4 flex items-center justify-between">
            <div className="label-section">Inspector</div>
            {selectedSpan && (
              <button
                onClick={() => setSelectedSpanForReplay(selectedSpan)}
                className="inline-flex items-center gap-1.5 rounded-pill bg-iris px-3 py-1.5 text-xs font-semibold text-window shadow-iris transition-[filter] hover:brightness-110"
              >
                <Play className="h-3.5 w-3.5" aria-hidden="true" />
                Mock Span
              </button>
            )}
          </div>

          {selectedSpan && (
            <>
              {/* Stat cards */}
              <div className="mb-4 grid grid-cols-3 gap-2">
                <StatCard label="Input" value={tokens ? tokens.input.toLocaleString() : '—'} />
                <StatCard label="Output" value={tokens ? tokens.output.toLocaleString() : '—'} />
                <StatCard
                  label="Cost"
                  value={cost != null ? `$${cost}` : '—'}
                  accent="text-iris-text"
                />
              </div>

              <TokenHeatmap tokenUsage={selectedSpan.token_usage} />
              <MemoryDiff span={selectedSpan} spans={spans} />

              <div className="mt-2 flex min-h-0 flex-1 flex-col">
                <div className="label-section mb-2">⤬ Attributes (Raw)</div>
                <JsonView value={selectedSpan} className="min-h-0 flex-1" />
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function StatCard({
  label,
  value,
  accent = 'text-ink-hi',
}: {
  label: string;
  value: string;
  accent?: string;
}) {
  return (
    <div className="rounded-pill border border-line bg-panel px-3 py-2.5">
      <div className="label-th mb-1">{label}</div>
      <div className={`font-mono text-[16px] font-semibold ${accent}`}>{value}</div>
    </div>
  );
}
