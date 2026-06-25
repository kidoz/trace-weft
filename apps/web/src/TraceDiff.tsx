import { useState, useEffect } from 'react';
import { ArrowLeft, GitCompareArrows } from 'lucide-react';
import type { Span } from './TraceDetail';
import { SpanKindBadge, StatusBadge } from './IconSystem';
import { apiUrl } from './api';

type DiffRow = {
  key: string;
  a: Span | null;
  b: Span | null;
};

// Align spans across the two traces by name+kind, falling back to positional
// index for unmatched spans. Intentionally simple and never throws on missing
// data: a span present only in A leaves `b` null (and vice versa).
function alignSpans(spansA: Span[], spansB: Span[]): DiffRow[] {
  const safeA = Array.isArray(spansA) ? spansA : [];
  const safeB = Array.isArray(spansB) ? spansB : [];

  const matchKey = (s: Span) => `${s.span_kind}::${s.name}`;
  const usedB = new Set<number>();
  const rows: DiffRow[] = [];

  // Build an index of B spans by match key (first-come wins per key).
  const bByKey = new Map<string, number[]>();
  safeB.forEach((s, i) => {
    const k = matchKey(s);
    const list = bByKey.get(k);
    if (list) list.push(i);
    else bByKey.set(k, [i]);
  });

  safeA.forEach((spanA, i) => {
    const k = matchKey(spanA);
    const candidates = bByKey.get(k);
    let matchIdx = -1;
    if (candidates) {
      const next = candidates.find((idx) => !usedB.has(idx));
      if (next !== undefined) matchIdx = next;
    }
    // Fall back to the same positional index if it's still free.
    if (matchIdx === -1 && i < safeB.length && !usedB.has(i) && matchKey(safeB[i]) === k) {
      matchIdx = i;
    }
    if (matchIdx !== -1) {
      usedB.add(matchIdx);
      rows.push({ key: `pair-${i}-${matchIdx}`, a: spanA, b: safeB[matchIdx] });
    } else {
      rows.push({ key: `a-${i}`, a: spanA, b: null });
    }
  });

  // Any B spans never matched are additions.
  safeB.forEach((spanB, i) => {
    if (!usedB.has(i)) rows.push({ key: `b-${i}`, a: null, b: spanB });
  });

  return rows;
}

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
}: {
  span: Span;
  other: Span | null;
  side: 'a' | 'b';
  added?: boolean;
}) {
  const latencyChanged = fieldChanged(span, other, (s) => s.latency_ms);
  const statusChanged = fieldChanged(span, other, (s) => s.status);
  const attrsChanged = fieldChanged(span, other, (s) => s.attributes);

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
  const [spansA, setSpansA] = useState<Span[]>([]);
  const [spansB, setSpansB] = useState<Span[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      fetch(apiUrl(`/api/traces/${traceA}`)).then((res) => res.json()),
      fetch(apiUrl(`/api/traces/${traceB}`)).then((res) => res.json()),
    ])
      .then(([dataA, dataB]) => {
        setSpansA(Array.isArray(dataA) ? dataA : []);
        setSpansB(Array.isArray(dataB) ? dataB : []);
        setLoading(false);
      })
      .catch((err) => {
        console.error('Failed to fetch traces for diff', err);
        setLoading(false);
      });
  }, [traceA, traceB]);

  if (loading) return <div className="p-6 text-ink-dim">Loading diff...</div>;

  const rows = alignSpans(spansA, spansB);
  const changedCount = rows.filter(
    (r) =>
      r.a &&
      r.b &&
      (fieldChanged(r.a, r.b, (s) => s.latency_ms) ||
        fieldChanged(r.a, r.b, (s) => s.status) ||
        fieldChanged(r.a, r.b, (s) => s.attributes)),
  ).length;
  const addedCount = rows.filter((r) => !r.a && r.b).length;

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
          {changedCount} changed · {addedCount} added
        </span>
      </div>

      <div className="flex flex-1 overflow-hidden rounded-panel border border-line">
        {/* Original (A) */}
        <div className="flex-1 overflow-y-auto bg-surface p-4">
          <div className="label-section mb-4">Original run {shortId(traceA)}</div>
          <div className="space-y-2">
            {rows.map((row) =>
              row.a ? (
                <SpanCard key={`${row.key}-a`} span={row.a} other={row.b} side="a" />
              ) : (
                <EmptyPlaceholder key={`${row.key}-a`} />
              ),
            )}
          </div>
        </div>

        {/* Replayed (B) */}
        <div className="flex-1 overflow-y-auto border-l border-line-inner bg-surface p-4">
          <div className="label-section mb-4">Replayed run {shortId(traceB)}</div>
          <div className="space-y-2">
            {rows.map((row) =>
              row.b ? (
                <SpanCard key={`${row.key}-b`} span={row.b} other={row.a} side="b" added={!row.a} />
              ) : (
                <EmptyPlaceholder key={`${row.key}-b`} />
              ),
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
