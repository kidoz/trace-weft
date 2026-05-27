import { useState, useEffect } from 'react';
import { ArrowLeft, GitCompareArrows, Network, Play, Rows3 } from 'lucide-react';
import { ReplayLab } from './ReplayLab';
import { TraceGraph } from './TraceGraph';
import { TokenHeatmap } from './TokenHeatmap';
import { MemoryDiff } from './MemoryDiff';
import { navigationIcons } from './IconRegistry';
import { MetricPill, SpanKindBadge } from './IconSystem';

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
  token_usage?: {
    input: number;
    output: number;
    reasoning: number | null;
    breakdown: Record<string, number>;
  } | null;
  memory_state?: Record<string, unknown> | null;
}

export function TraceDetail({ traceId, onBack }: { traceId: string; onBack: () => void }) {
  const [spans, setSpans] = useState<Span[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedSpanForReplay, setSelectedSpanForReplay] = useState<Span | null>(null);
  const [viewMode, setViewMode] = useState<'tree' | 'graph'>('tree');
  const [selectedSpan, setSelectedSpan] = useState<Span | null>(null);

  useEffect(() => {
    fetch(`http://127.0.0.1:3000/api/traces/${traceId}`)
      .then((res) => res.json())
      .then((data: Span[]) => {
        setSpans(data);
        if (data.length > 0) {
          setSelectedSpan(data[0]);
        }
        setLoading(false);
      })
      .catch((err: unknown) => {
        console.error('Failed to fetch trace', err);
        setLoading(false);
      });
  }, [traceId]);

  if (loading) return <div className="p-8 text-slate-500">Loading spans...</div>;

  if (selectedSpanForReplay) {
    return <ReplayLab span={selectedSpanForReplay} onBack={() => setSelectedSpanForReplay(null)} />;
  }

  return (
    <div className="mx-auto flex h-screen max-w-7xl flex-col p-6">
      <div className="mb-5 flex items-center justify-between">
        <div className="flex items-center">
          <button
            onClick={onBack}
            className="mr-4 inline-flex items-center gap-2 rounded border border-slate-200 bg-white px-3 py-2 text-sm font-medium text-slate-600 transition-colors hover:text-slate-950"
          >
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
            Back
          </button>
          <div>
            <h1 className="text-xl font-bold text-slate-900">Trace Detail</h1>
            <div className="mt-1 flex flex-wrap items-center gap-2">
              <span className="font-mono text-xs text-slate-500">{traceId}</span>
              <MetricPill icon={navigationIcons.traces} label={`${spans.length} spans`} />
            </div>
          </div>
        </div>
        <div className="flex rounded border border-slate-200 bg-slate-100 p-1">
          <button
            onClick={() => setViewMode('tree')}
            className={`inline-flex items-center gap-2 rounded px-3 py-1.5 text-sm font-medium transition-colors ${viewMode === 'tree' ? 'bg-white text-slate-950 shadow-sm' : 'text-slate-600 hover:text-slate-900'}`}
          >
            <Rows3 className="h-4 w-4" aria-hidden="true" />
            Tree
          </button>
          <button
            onClick={() => setViewMode('graph')}
            className={`inline-flex items-center gap-2 rounded px-3 py-1.5 text-sm font-medium transition-colors ${viewMode === 'graph' ? 'bg-white text-slate-950 shadow-sm' : 'text-slate-600 hover:text-slate-900'}`}
          >
            <Network className="h-4 w-4" aria-hidden="true" />
            Graph
          </button>
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden rounded border border-slate-200 bg-white shadow-sm">
        {/* Left pane: Span Tree or Graph */}
        <div className="w-2/3 border-r border-slate-200 flex flex-col relative">
          {viewMode === 'tree' ? (
            <div className="p-4 overflow-y-auto bg-slate-50/50 flex-1">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-4">
                Span Tree
              </h2>
              <div className="space-y-2">
                {spans.map((span) => (
                  <div
                    key={span.span_id}
                    onClick={() => setSelectedSpan(span)}
                    className={`p-3 bg-white border rounded shadow-sm cursor-pointer transition-colors group ${selectedSpan?.span_id === span.span_id ? 'border-blue-500 ring-1 ring-blue-200' : 'border-slate-200 hover:border-blue-300'}`}
                  >
                    <div className="flex items-center justify-between mb-1">
                      <div className="flex min-w-0 items-center gap-2">
                        <SpanKindBadge kind={span.span_kind} />
                        <span className="truncate text-sm font-medium text-slate-800">
                          {span.name}
                        </span>
                      </div>
                      <div className="flex items-center gap-2">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setSelectedSpanForReplay(span);
                          }}
                          className="inline-flex items-center gap-1 rounded border border-sky-200 px-2 py-0.5 text-[10px] font-bold uppercase text-sky-700 opacity-0 transition-all hover:bg-sky-50 group-hover:opacity-100"
                        >
                          <Play className="h-3 w-3" aria-hidden="true" />
                          Mock
                        </button>
                      </div>
                    </div>
                    <div className="flex justify-between text-xs text-slate-500">
                      <span>{span.latency_ms}ms</span>
                      <span className={span.status === 'ok' ? 'text-green-600' : 'text-red-600'}>
                        {span.status}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="flex-1 relative">
              <TraceGraph spans={spans} onSpanClick={setSelectedSpan} />
            </div>
          )}
        </div>

        {/* Right pane: Inspector */}
        <div className="w-1/3 p-6 overflow-y-auto flex flex-col bg-white">
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-500">
              Inspector
            </h2>
            {selectedSpan && (
              <button
                onClick={() => setSelectedSpanForReplay(selectedSpan)}
                className="inline-flex items-center gap-1.5 rounded border border-sky-200 px-2 py-1 text-xs font-bold uppercase text-sky-700 transition-all hover:bg-sky-50"
              >
                <Play className="h-3.5 w-3.5" aria-hidden="true" />
                Mock Span
              </button>
            )}
          </div>

          {selectedSpan && (
            <>
              <TokenHeatmap tokenUsage={selectedSpan.token_usage} />
              <MemoryDiff span={selectedSpan} spans={spans} />

              <div className="flex flex-col flex-1 min-h-0">
                <h3 className="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-2">
                  <span className="inline-flex items-center gap-2">
                    <GitCompareArrows className="h-4 w-4" aria-hidden="true" />
                    Attributes (Raw)
                  </span>
                </h3>
                <pre className="bg-slate-900 text-slate-100 p-4 rounded text-xs overflow-x-auto flex-1 min-h-0">
                  {JSON.stringify(selectedSpan, null, 2)}
                </pre>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
