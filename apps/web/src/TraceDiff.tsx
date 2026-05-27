import { useState, useEffect } from 'react';
import { ArrowLeft, GitCompareArrows } from 'lucide-react';
import type { Span } from './TraceDetail';
import { SpanKindBadge, StatusBadge } from './IconSystem';

export function TraceDiff({
  traceA,
  traceB,
  onBack,
}: {
  traceA: string;
  traceB: string;
  onBack: () => void;
}) {
  const [spansA, setSpansA] = useState<Span[]>([]);
  const [spansB, setSpansB] = useState<Span[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      fetch(`http://127.0.0.1:3000/api/traces/${traceA}`).then((res) => res.json()),
      fetch(`http://127.0.0.1:3000/api/traces/${traceB}`).then((res) => res.json()),
    ])
      .then(([dataA, dataB]) => {
        setSpansA(dataA);
        setSpansB(dataB);
        setLoading(false);
      })
      .catch((err) => {
        console.error('Failed to fetch traces for diff', err);
        setLoading(false);
      });
  }, [traceA, traceB]);

  if (loading) return <div className="p-8 text-slate-500">Loading diff...</div>;

  return (
    <div className="p-8 flex flex-col h-screen max-w-7xl mx-auto">
      <div className="flex items-center mb-6">
        <button
          onClick={onBack}
          className="mr-4 inline-flex items-center gap-2 rounded border border-slate-200 bg-white px-3 py-2 text-sm font-medium text-slate-600 transition-colors hover:text-slate-950"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          Back
        </button>
        <h1 className="inline-flex items-center gap-2 text-xl font-bold text-slate-900">
          <GitCompareArrows className="h-5 w-5 text-orange-700" aria-hidden="true" />
          Trace Diff: <span className="font-mono text-sm text-blue-600">{traceA}</span> vs{' '}
          <span className="font-mono text-sm text-green-600">{traceB}</span>
        </h1>
      </div>

      <div className="flex flex-1 overflow-hidden rounded border border-slate-200 bg-white shadow-sm">
        {/* Trace A */}
        <div className="flex-1 border-r border-slate-200 p-4 overflow-y-auto">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-4">
            Original Run ({traceA.slice(0, 8)})
          </h2>
          <div className="space-y-2">
            {spansA.map((span) => (
              <div
                key={span.span_id}
                className="p-3 bg-slate-50 border border-slate-200 rounded text-sm"
              >
                <div className="mb-2 flex flex-wrap items-center gap-2 font-medium text-slate-800">
                  <SpanKindBadge kind={span.span_kind} />
                  <span>{span.name}</span>
                </div>
                <div className="mb-1">
                  <StatusBadge status={span.status} />
                </div>
                <div className="text-xs text-slate-600">Latency: {span.latency_ms}ms</div>
                <pre className="mt-2 p-2 bg-white rounded border border-slate-100 text-[10px] overflow-x-auto text-slate-600">
                  {JSON.stringify(span.attributes, null, 2)}
                </pre>
              </div>
            ))}
          </div>
        </div>

        {/* Trace B */}
        <div className="flex-1 p-4 overflow-y-auto">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-4">
            Replayed/Forked Run ({traceB.slice(0, 8)})
          </h2>
          <div className="space-y-2">
            {spansB.map((span) => (
              <div
                key={span.span_id}
                className="p-3 bg-slate-50 border border-slate-200 rounded text-sm"
              >
                <div className="mb-2 flex flex-wrap items-center gap-2 font-medium text-slate-800">
                  <SpanKindBadge kind={span.span_kind} />
                  <span>{span.name}</span>
                </div>
                <div className="mb-1">
                  <StatusBadge status={span.status} />
                </div>
                <div className="text-xs text-slate-600">Latency: {span.latency_ms}ms</div>
                <pre className="mt-2 p-2 bg-white rounded border border-slate-100 text-[10px] overflow-x-auto text-slate-600">
                  {JSON.stringify(span.attributes, null, 2)}
                </pre>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
