import { useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowLeft, GitCompareArrows } from 'lucide-react';
import { api, queryKeys, type Span } from './api';
import { SpanKindBadge, StatusBadge } from './IconSystem';

function shortId(id: string): string {
  return id ? id.slice(0, 8) : '—';
}

function fieldChanged(a: Span | null, b: Span | null, pick: (s: Span) => unknown): boolean {
  if (!a || !b) return false;
  return JSON.stringify(pick(a)) !== JSON.stringify(pick(b));
}

const FIELD_BASE = 'font-mono text-[12px] text-ink-mid leading-relaxed';
const OLD_ACCENT = 'bg-[rgba(251,113,133,0.12)] border-l-2 border-error pl-2';
const NEW_ACCENT = 'bg-[rgba(74,222,128,0.12)] border-l-2 border-ok pl-2';

function FieldLine({
  changed,
  side,
  children,
}: {
  changed: boolean;
  side: 'a' | 'b';
  children: React.ReactNode;
}) {
  const accent = changed ? (side === 'a' ? OLD_ACCENT : NEW_ACCENT) : '';
  return <div className={`${FIELD_BASE} ${accent}`}>{children}</div>;
}

function SpanCard({
  span,
  other,
  side,
  added = false,
  changedFields = [],
}: {
  span: Span;
  other: Span | null;
  side: 'a' | 'b';
  added?: boolean;
  changedFields?: string[];
}) {
  const latencyChanged =
    changedFields.includes('latency_ms') || fieldChanged(span, other, (s) => s.latency_ms);
  const statusChanged =
    changedFields.includes('status') || fieldChanged(span, other, (s) => s.status);
  const attrsChanged =
    changedFields.includes('attributes') || fieldChanged(span, other, (s) => s.attributes);

  const border = added ? 'border-ok' : 'border-line-inner';

  return (
    <div className={`bg-panel-2 border ${border} rounded-panel p-3`}>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <SpanKindBadge kind={span.span_kind} />
        <span className="text-[13.5px] font-medium text-ink-hi">{span.name}</span>
        {added && (
          <span className="ml-auto font-mono text-[10px] font-bold uppercase tracking-wide text-ok">
            + Added
          </span>
        )}
      </div>

      <div className="space-y-1">
        <FieldLine changed={latencyChanged} side={side}>
          latency {span.latency_ms ?? '—'}ms
        </FieldLine>
        <div className={statusChanged ? `${side === 'a' ? OLD_ACCENT : NEW_ACCENT} py-0.5` : ''}>
          <StatusBadge status={span.status} />
        </div>
      </div>

      <pre
        className={`mt-2 overflow-x-auto rounded-panel border border-line-inner bg-code p-2 font-mono text-[11px] text-ink-mid ${
          attrsChanged ? (side === 'a' ? OLD_ACCENT : NEW_ACCENT) : ''
        }`}
      >
        {JSON.stringify(span.attributes, null, 2)}
      </pre>
    </div>
  );
}

function EmptyPlaceholder() {
  return (
    <div className="flex items-center justify-center rounded-panel border border-dashed border-line-node p-6 text-[12px] font-mono text-ink-faint">
      — no matching span —
    </div>
  );
}

export function TraceDiff({
  traceA,
  traceB,
  onBack,
}: {
  traceA: string;
  traceB: string;
  onBack: () => void;
}) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const { data, isLoading, error } = useQuery({
    queryKey: queryKeys.traceDiff(traceA, traceB),
    queryFn: () => api.getTraceDiff(traceA, traceB),
  });
  const rows = data?.rows ?? [];
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 148,
    overscan: 8,
  });
  const summary = data?.summary;
  const changedCount = summary?.changed ?? 0;
  const addedCount = summary?.added ?? 0;
  const removedCount = summary?.removed ?? 0;

  if (isLoading) return <div className="p-6 text-ink-dim">Loading diff...</div>;
  if (error) return <div className="p-6 text-error">{error.message}</div>;

  return (
    <div className="flex h-screen max-w-7xl flex-col mx-auto p-6">
      <div className="mb-6 flex items-center gap-3">
        <button
          onClick={onBack}
          className="inline-flex items-center gap-2 rounded-pill border border-line bg-panel px-3 py-1.5 text-sm font-medium text-ink-mid transition-colors hover:text-ink-hi"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          Back
        </button>

        <h1 className="inline-flex items-center gap-2 text-[24px] font-bold text-ink-hi">
          <GitCompareArrows className="h-6 w-6" style={{ color: '#fb923c' }} aria-hidden="true" />
          Trace Diff
        </h1>

        <span className="rounded-chip border border-[rgba(251,113,133,0.25)] bg-[rgba(251,113,133,0.12)] px-2 py-0.5 font-mono text-[12px] text-error">
          {shortId(traceA)}
        </span>
        <span className="text-[12px] text-ink-dim">vs</span>
        <span className="rounded-chip border border-[rgba(74,222,128,0.25)] bg-[rgba(74,222,128,0.12)] px-2 py-0.5 font-mono text-[12px] text-ok">
          {shortId(traceB)}
        </span>

        <span className="ml-auto font-mono text-[12px] text-warn">
          {changedCount} changed · {addedCount} added · {removedCount} removed
        </span>
      </div>

      <div ref={parentRef} className="flex flex-1 overflow-auto rounded-panel border border-line">
        <div className="relative flex w-full" style={{ height: `${virtualizer.getTotalSize()}px` }}>
          {/* Original (A) */}
          <div className="w-1/2 bg-surface p-4">
            <div className="label-section mb-4">Original run {shortId(traceA)}</div>
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const row = rows[virtualRow.index];
              return (
                <div
                  key={`${row.key}-a`}
                  className="absolute left-4 right-[calc(50%+8px)]"
                  style={{ transform: `translateY(${virtualRow.start + 34}px)` }}
                >
                  {row.a ? (
                    <SpanCard
                      span={row.a}
                      other={row.b}
                      side="a"
                      changedFields={row.changed_fields}
                    />
                  ) : (
                    <EmptyPlaceholder />
                  )}
                </div>
              );
            })}
          </div>

          {/* Replayed (B) */}
          <div className="w-1/2 border-l border-line-inner bg-surface p-4">
            <div className="label-section mb-4">Replayed run {shortId(traceB)}</div>
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const row = rows[virtualRow.index];
              return (
                <div
                  key={`${row.key}-b`}
                  className="absolute left-[calc(50%+8px)] right-4"
                  style={{ transform: `translateY(${virtualRow.start + 34}px)` }}
                >
                  {row.b ? (
                    <SpanCard
                      span={row.b}
                      other={row.a}
                      side="b"
                      added={!row.a}
                      changedFields={row.changed_fields}
                    />
                  ) : (
                    <EmptyPlaceholder />
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
