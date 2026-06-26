import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowLeft, Braces, Clock3, Download, FileText, Network, Play, Rows3 } from 'lucide-react';
import { ReplayLab } from './ReplayLab';
import { TraceGraph } from './TraceGraph';
import { TokenHeatmap } from './TokenHeatmap';
import { MemoryDiff } from './MemoryDiff';
import { JsonView } from './JsonView';
import { SpanKindBadge, StatusBadge } from './IconSystem';
import { spanKindColor } from './spanColors';
import { api, queryKeys, type BlobRef, type Span, type TraceEvent } from './api';

type ViewMode = 'waterfall' | 'graph' | 'transcript' | 'content';

const EMPTY_SPANS: Span[] = [];
const EMPTY_EVENTS: TraceEvent[] = [];

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

function shortId(id: string): string {
  return id.slice(0, 8);
}

function hasBlob(ref: BlobRef | null | undefined): ref is BlobRef {
  return Boolean(ref?.hash);
}

function spanBlobRefs(span: Span | null): Array<{ label: string; ref: BlobRef }> {
  if (!span) return [];
  const refs: Array<{ label: string; ref: BlobRef }> = [];
  if (hasBlob(span.input_ref)) refs.push({ label: 'Input', ref: span.input_ref });
  if (hasBlob(span.output_ref)) refs.push({ label: 'Output', ref: span.output_ref });
  span.retrieved_document_refs?.forEach((ref, index) => {
    if (hasBlob(ref)) refs.push({ label: `Retrieval ${index + 1}`, ref });
  });
  return refs;
}

export function TraceDetail({ traceId, onBack }: { traceId: string; onBack: () => void }) {
  const [selectedSpanForReplay, setSelectedSpanForReplay] = useState<Span | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>('waterfall');
  const [selectedSpan, setSelectedSpan] = useState<Span | null>(null);
  const waterfallRef = useRef<HTMLDivElement | null>(null);

  const spansQuery = useQuery({
    queryKey: queryKeys.trace(traceId),
    queryFn: () => api.getTrace(traceId),
  });
  const eventsQuery = useQuery({
    queryKey: queryKeys.traceEvents(traceId),
    queryFn: () => api.getTraceEvents(traceId),
  });

  const spans = spansQuery.data ?? EMPTY_SPANS;
  const events = eventsQuery.data ?? EMPTY_EVENTS;

  useEffect(() => {
    if (spans.length === 0) {
      setSelectedSpan(null);
      return;
    }
    if (!selectedSpan || !spans.some((span) => span.span_id === selectedSpan.span_id)) {
      setSelectedSpan(spans[0] ?? null);
    }
  }, [selectedSpan, spans]);

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

  const waterfallVirtualizer = useVirtualizer({
    count: spans.length,
    getScrollElement: () => waterfallRef.current,
    estimateSize: () => 68,
    overscan: 10,
  });

  if (spansQuery.isLoading || eventsQuery.isLoading) {
    return <div className="p-8 text-ink-dim">Loading spans...</div>;
  }
  const loadError = spansQuery.error ?? eventsQuery.error;
  if (loadError) return <div className="p-8 text-error">{loadError.message}</div>;

  if (selectedSpanForReplay) {
    return <ReplayLab span={selectedSpanForReplay} onBack={() => setSelectedSpanForReplay(null)} />;
  }

  const viewToggle = (mode: ViewMode, label: string, Icon: typeof Rows3) => (
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
  const selectedBlobRefs = spanBlobRefs(selectedSpan);
  const traceStart = Number.isFinite(window.min) ? window.min : 0;
  const traceEnd = traceStart + window.total;

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
          {viewToggle('waterfall', 'Waterfall', Rows3)}
          {viewToggle('graph', 'Graph', Network)}
          {viewToggle('transcript', 'Transcript', FileText)}
          {viewToggle('content', 'Content', Braces)}
        </div>
      </div>

      {/* Body: tree/graph + inspector */}
      <div className="flex min-h-0 flex-1 overflow-hidden rounded-panel border border-line bg-surface shadow-window">
        {/* Left pane */}
        <div className="relative flex min-w-0 flex-1 flex-col border-r border-line-inner">
          {viewMode === 'waterfall' ? (
            <div ref={waterfallRef} className="flex-1 overflow-y-auto bg-surface p-4">
              <div className="mb-3 flex items-center justify-between">
                <div className="label-section">Waterfall</div>
                <div className="font-mono text-[11px] text-ink-dim">
                  {new Date(traceStart).toLocaleTimeString()} -{' '}
                  {new Date(traceEnd).toLocaleTimeString()}
                </div>
              </div>
              <div className="mb-3 grid grid-cols-[210px_1fr_80px] gap-3 px-3">
                <div />
                <div className="relative h-5 border-t border-line-inner">
                  <span className="absolute -top-2 left-0 bg-surface pr-1 font-mono text-[10px] text-ink-dim">
                    0ms
                  </span>
                  <span className="absolute -top-2 left-1/2 -translate-x-1/2 bg-surface px-1 font-mono text-[10px] text-ink-dim">
                    {Math.round(window.total / 2)}ms
                  </span>
                  <span className="absolute -top-2 right-0 bg-surface pl-1 font-mono text-[10px] text-ink-dim">
                    {Math.round(window.total)}ms
                  </span>
                </div>
                <div />
              </div>
              <div
                className="relative"
                style={{ height: `${waterfallVirtualizer.getTotalSize()}px` }}
              >
                {waterfallVirtualizer.getVirtualItems().map((virtualRow) => {
                  const span = spans[virtualRow.index];
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
                      className="absolute left-0 top-0 grid w-full cursor-pointer grid-cols-[210px_1fr_80px] items-center gap-3 rounded-panel border p-3 transition-colors"
                      style={{
                        backgroundColor: selected ? 'rgba(124,131,255,0.08)' : '#13161b',
                        borderColor: selected
                          ? 'rgba(124,131,255,0.35)'
                          : pending
                            ? 'rgba(251,191,36,0.5)'
                            : '#20242c',
                        borderStyle: pending ? 'dashed' : 'solid',
                        height: `${virtualRow.size - 8}px`,
                        transform: `translateY(${virtualRow.start}px)`,
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
          ) : viewMode === 'graph' ? (
            <div className="relative flex-1">
              <TraceGraph spans={spans} onSpanClick={setSelectedSpan} />
            </div>
          ) : viewMode === 'transcript' ? (
            <TranscriptView events={events} spans={spans} onSelectSpan={setSelectedSpan} />
          ) : (
            <ContentView span={selectedSpan} />
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

              <InspectorSections span={selectedSpan} />
              <TokenHeatmap tokenUsage={selectedSpan.token_usage} />
              <MemoryDiff span={selectedSpan} spans={spans} />

              {selectedBlobRefs.length > 0 && (
                <div className="mb-5">
                  <div className="label-section mb-2">Content refs</div>
                  <div className="space-y-2">
                    {selectedBlobRefs.map(({ label, ref }) => (
                      <button
                        key={`${label}-${ref.hash}`}
                        onClick={() => setViewMode('content')}
                        className="flex w-full items-center justify-between gap-3 rounded-panel border border-line-inner bg-panel px-3 py-2 text-left transition-colors hover:border-[rgba(124,131,255,0.45)]"
                      >
                        <span className="text-xs font-semibold text-ink-hi">{label}</span>
                        <span className="truncate font-mono text-[11px] text-iris-text">
                          {shortId(ref.hash)}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              )}

              <div className="mt-2 flex min-h-0 flex-1 flex-col">
                <div className="label-section mb-2">Attributes (Raw)</div>
                <JsonView value={selectedSpan} className="min-h-0 flex-1" />
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function InspectorSections({ span }: { span: Span }) {
  const modelLabel =
    span.model_provider || span.model_name
      ? [span.model_provider, span.model_name].filter(Boolean).join(' / ')
      : '—';
  const hasError = Boolean(span.error_type || span.error_message_redacted || span.status_message);
  const hasModel = Boolean(
    span.model_provider || span.model_name || span.prompt_template_id || span.prompt_version,
  );
  const hasTool = Boolean(span.tool_name || span.tool_schema_hash);
  const hasRetrieval = Boolean(span.retrieval_query_hash || span.retrieved_document_refs?.length);
  const hasTelemetry =
    Object.keys(span.otel_attributes ?? {}).length > 0 ||
    Object.keys(span.openinference_attributes ?? {}).length > 0;

  return (
    <div className="mb-5 space-y-3">
      <InspectorGroup title="Span metadata">
        <KeyValue label="Status" value={<StatusBadge status={span.status} />} />
        <KeyValue label="Span ID" value={span.span_id} mono />
        <KeyValue label="Run ID" value={span.run_id} mono />
        {span.parent_span_id && <KeyValue label="Parent" value={span.parent_span_id} mono />}
        {span.session_id && <KeyValue label="Session" value={span.session_id} mono />}
        {span.redaction_policy && <KeyValue label="Capture" value={span.redaction_policy} mono />}
        {span.retry_count != null && <KeyValue label="Retries" value={span.retry_count} mono />}
        {span.cache_hit != null && (
          <KeyValue label="Cache hit" value={String(span.cache_hit)} mono />
        )}
      </InspectorGroup>

      {hasError && (
        <InspectorGroup title="Status and error">
          {span.status_message && <KeyValue label="Message" value={span.status_message} />}
          {span.error_type && <KeyValue label="Type" value={span.error_type} mono />}
          {span.error_message_redacted && (
            <KeyValue label="Redacted error" value={span.error_message_redacted} />
          )}
        </InspectorGroup>
      )}

      {hasModel && (
        <InspectorGroup title="Model and prompt">
          <KeyValue label="Model" value={modelLabel} mono />
          {span.prompt_template_id && (
            <KeyValue label="Template" value={span.prompt_template_id} mono />
          )}
          {span.prompt_version && <KeyValue label="Version" value={span.prompt_version} mono />}
        </InspectorGroup>
      )}

      {hasTool && (
        <InspectorGroup title="Tool">
          {span.tool_name && <KeyValue label="Name" value={span.tool_name} mono />}
          {span.tool_schema_hash && <KeyValue label="Schema" value={span.tool_schema_hash} mono />}
        </InspectorGroup>
      )}

      {hasRetrieval && (
        <InspectorGroup title="Retrieval">
          {span.retrieval_query_hash && (
            <KeyValue label="Query hash" value={span.retrieval_query_hash} mono />
          )}
          <KeyValue label="Documents" value={span.retrieved_document_refs?.length ?? 0} mono />
        </InspectorGroup>
      )}

      {hasTelemetry && (
        <InspectorGroup title="Telemetry attrs">
          <KeyValue
            label="OTel"
            value={`${Object.keys(span.otel_attributes ?? {}).length} keys`}
            mono
          />
          <KeyValue
            label="OpenInf"
            value={`${Object.keys(span.openinference_attributes ?? {}).length} keys`}
            mono
          />
        </InspectorGroup>
      )}
    </div>
  );
}

function InspectorGroup({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="rounded-panel border border-line-inner bg-panel p-3">
      <div className="label-section mb-2">{title}</div>
      <div className="space-y-1.5">{children}</div>
    </div>
  );
}

function KeyValue({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="grid grid-cols-[96px_1fr] gap-2 text-xs">
      <div className="text-ink-dim">{label}</div>
      <div className={`min-w-0 truncate text-ink-hi ${mono ? 'font-mono' : ''}`}>{value}</div>
    </div>
  );
}

function TranscriptView({
  events,
  spans,
  onSelectSpan,
}: {
  events: TraceEvent[];
  spans: Span[];
  onSelectSpan: (span: Span) => void;
}) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const spanById = useMemo(() => new Map(spans.map((span) => [span.span_id, span])), [spans]);
  const items = useMemo(() => {
    const spanItems = spans.map((span) => ({
      kind: 'span' as const,
      id: span.span_id,
      time: span.start_time,
      span,
      event: null,
    }));
    const eventItems = events.map((event) => ({
      kind: 'event' as const,
      id: event.event_id,
      time: event.timestamp,
      span: event.parent_span_id ? (spanById.get(event.parent_span_id) ?? null) : null,
      event,
    }));
    return [...spanItems, ...eventItems].sort((a, b) => a.time - b.time);
  }, [events, spanById, spans]);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 82,
    overscan: 12,
  });

  return (
    <div ref={parentRef} className="flex-1 overflow-y-auto bg-surface p-4">
      <div className="mb-3 flex items-center justify-between">
        <div className="label-section">Transcript</div>
        <span className="font-mono text-[11px] text-ink-dim">
          {events.length} events · {spans.length} spans
        </span>
      </div>

      {items.length === 0 ? (
        <div className="rounded-panel border border-dashed border-line-node p-8 text-center text-ink-dim">
          No transcript items recorded.
        </div>
      ) : (
        <div className="relative" style={{ height: `${virtualizer.getTotalSize()}px` }}>
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const item = items[virtualRow.index];
            return item.kind === 'span' ? (
              <button
                key={item.id}
                onClick={() => onSelectSpan(item.span)}
                className="absolute left-0 top-0 grid w-full grid-cols-[130px_1fr_90px] items-start gap-3 rounded-panel border border-line-inner bg-panel p-3 text-left transition-colors hover:border-[rgba(124,131,255,0.45)]"
                style={{
                  minHeight: `${virtualRow.size - 8}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                <div className="font-mono text-[11px] text-ink-dim">
                  {new Date(item.time).toLocaleTimeString()}
                </div>
                <div className="min-w-0">
                  <div className="mb-1 flex min-w-0 items-center gap-2">
                    <SpanKindBadge kind={item.span.span_kind} />
                    <span className="truncate text-sm font-semibold text-ink-hi">
                      {item.span.name}
                    </span>
                  </div>
                  <div className="truncate font-mono text-[11px] text-ink-dim">
                    {item.span.span_id}
                  </div>
                </div>
                <StatusBadge status={item.span.status} />
              </button>
            ) : (
              <div
                key={item.id}
                className="absolute left-0 top-0 grid w-full grid-cols-[130px_1fr] items-start gap-3 rounded-panel border border-line-inner bg-panel-2 p-3"
                style={{
                  minHeight: `${virtualRow.size - 8}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                <div className="font-mono text-[11px] text-ink-dim">
                  {new Date(item.time).toLocaleTimeString()}
                </div>
                <div className="min-w-0">
                  <div className="mb-1 flex flex-wrap items-center gap-2">
                    <span className="rounded-chip border border-[rgba(86,207,225,0.25)] bg-[rgba(86,207,225,0.10)] px-2 py-0.5 text-[10px] font-bold uppercase tracking-wide text-flow">
                      {item.event.event_kind}
                    </span>
                    <span className="text-sm font-semibold text-ink-hi">{item.event.name}</span>
                    {item.span && (
                      <button
                        onClick={() => onSelectSpan(item.span!)}
                        className="font-mono text-[11px] text-iris-text hover:underline"
                      >
                        {shortId(item.span.span_id)}
                      </button>
                    )}
                  </div>
                  {Object.keys(item.event.attributes).length > 0 && (
                    <JsonView value={item.event.attributes} className="max-h-48" />
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function ContentView({ span }: { span: Span | null }) {
  const refs = spanBlobRefs(span);

  return (
    <div className="flex-1 overflow-y-auto bg-surface p-4">
      <div className="mb-3 flex items-center justify-between">
        <div className="label-section">Content</div>
        {span && (
          <span className="font-mono text-[11px] text-ink-dim">{shortId(span.span_id)}</span>
        )}
      </div>

      {!span ? (
        <div className="rounded-panel border border-dashed border-line-node p-8 text-center text-ink-dim">
          Select a span.
        </div>
      ) : refs.length === 0 ? (
        <div className="rounded-panel border border-dashed border-line-node p-8 text-center text-ink-dim">
          No content refs on selected span.
        </div>
      ) : (
        <div className="space-y-4">
          {refs.map(({ label, ref }) => (
            <BlobPreview key={`${label}-${ref.hash}`} label={label} blob={ref} />
          ))}
        </div>
      )}
    </div>
  );
}

function BlobPreview({ label, blob }: { label: string; blob: BlobRef }) {
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadBlob = async () => {
    setLoading(true);
    setError(null);
    try {
      const text = await api.getBlobText(blob.hash);
      setContent(text.length > 12000 ? `${text.slice(0, 12000)}\n...` : text);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to load blob');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="rounded-panel border border-line-inner bg-panel p-4">
      <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-sm font-semibold text-ink-hi">{label}</span>
            <span className="rounded-chip border border-line-node bg-panel-2 px-2 py-0.5 font-mono text-[10px] uppercase text-ink-mid">
              {blob.redaction_status}
            </span>
          </div>
          <div className="mt-1 truncate font-mono text-[11px] text-iris-text">{blob.hash}</div>
        </div>
        <button
          onClick={() => void loadBlob()}
          disabled={loading}
          className="inline-flex items-center gap-2 rounded-pill border border-line bg-nav px-3 py-1.5 text-xs font-semibold text-ink-mid transition-colors hover:text-ink-hi disabled:opacity-50"
        >
          <Download className="h-3.5 w-3.5" aria-hidden="true" />
          {loading ? 'Loading' : 'Load blob'}
        </button>
      </div>

      <div className="mb-3 grid grid-cols-3 gap-2">
        <StatCard label="Type" value={blob.content_type || '—'} />
        <StatCard label="Size" value={`${blob.size_bytes.toLocaleString()} B`} />
        <StatCard label="Store" value={blob.storage_backend || '—'} />
      </div>

      {blob.preview_text_redacted && (
        <div className="mb-3">
          <div className="label-section mb-2">Redacted preview</div>
          <pre className="max-h-48 overflow-auto rounded-panel border border-line-inner bg-code p-3 font-mono text-xs text-jsonstr">
            {blob.preview_text_redacted}
          </pre>
        </div>
      )}

      {error && <div className="text-sm text-error">{error}</div>}
      {content && (
        <div>
          <div className="label-section mb-2">Blob body</div>
          <pre className="max-h-80 overflow-auto rounded-panel border border-line-inner bg-code p-3 font-mono text-xs text-ink-hi">
            {content}
          </pre>
        </div>
      )}
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
