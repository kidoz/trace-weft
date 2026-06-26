import type {
  EvalSummary,
  ReplayConfigRequest,
  ReplayConfigResponse,
  ReplayPlan,
  Span,
  TraceDiffResponse,
  TraceEvent,
  TraceSummary,
} from './generated/api-types';

// Base URL for the TraceWeft API.
//
// Empty by default so the app uses same-origin relative `/api/...` paths: the
// Vite dev server proxies those to the API (see `vite.config.ts`), and a
// same-origin deployment serves them directly. The desktop build sets
// `VITE_API_BASE` to the embedded server's absolute origin
// (`http://127.0.0.1:3000`) since its webview is not same-origin with the API.
export const API_BASE = import.meta.env.VITE_API_BASE ?? '';

/** Resolve an API path (e.g. `/api/traces`) against the configured base. */
export const apiUrl = (path: string): string => `${API_BASE}${path}`;

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(apiUrl(path));
  if (!res.ok) throw new Error(`${path} request failed: ${res.status}`);
  return (await res.json()) as T;
}

async function postJson<TResponse>(path: string, body: unknown): Promise<TResponse> {
  const res = await fetch(apiUrl(path), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${path} request failed: ${res.status}`);
  return (await res.json()) as TResponse;
}

export const queryKeys = {
  traces: ['traces'] as const,
  trace: (traceId: string) => ['trace', traceId] as const,
  traceEvents: (traceId: string) => ['trace-events', traceId] as const,
  traceDiff: (traceA: string, traceB: string) => ['trace-diff', traceA, traceB] as const,
  evals: ['evals'] as const,
  hitlPending: ['hitl-pending'] as const,
  replayPlan: (traceId: string, spanId: string) => ['replay-plan', traceId, spanId] as const,
};

export const api = {
  listTraces: () => getJson<TraceSummary[]>('/api/traces'),
  getTrace: (traceId: string) => getJson<Span[]>(`/api/traces/${traceId}`),
  getTraceEvents: (traceId: string) => getJson<TraceEvent[]>(`/api/traces/${traceId}/events`),
  getTraceDiff: (traceA: string, traceB: string) =>
    getJson<TraceDiffResponse>(`/api/diff/${traceA}/${traceB}`),
  listEvals: () => getJson<EvalSummary[]>('/api/evals'),
  getPendingApprovals: () => getJson<string[]>('/api/hitl/pending'),
  resolveApproval: (body: {
    span_id: string;
    action: 'approve' | 'reject';
    value?: unknown;
    reason?: string;
  }) => postJson<unknown>('/api/hitl/resolve', body),
  getReplayPlan: (traceId: string, spanId: string) =>
    getJson<ReplayPlan>(`/api/traces/${traceId}/replay-plan/${spanId}`),
  generateReplayConfig: (body: ReplayConfigRequest) =>
    postJson<ReplayConfigResponse>('/api/replay/config', body),
};

export type {
  BlobRef,
  EvalSummary,
  ReplayConfigRequest,
  ReplayConfigResponse,
  ReplayPlan,
  Span,
  TraceDiffResponse,
  TraceDiffRow,
  TraceEvent,
  TraceSummary,
} from './generated/api-types';
