import type { Span } from './TraceDetail';

export function MemoryDiff({ span, spans }: { span: Span; spans: Span[] }) {
  if (!span.memory_state) return null;

  // Try to find the parent span's memory state to show a diff
  // For a proper diff we might want a library, but for MVP we will show side by side
  let previousState = null;
  if (span.parent_span_id) {
    const parent = spans.find((s) => s.span_id === span.parent_span_id);
    if (parent?.memory_state) {
      previousState = parent.memory_state;
    }
  }

  return (
    <div className="mb-6 flex min-h-0 flex-1 flex-col">
      <h3 className="label-section mb-2">Memory State (Scratchpad)</h3>
      {previousState ? (
        <div className="flex min-h-0 flex-1 gap-2">
          <div className="flex min-h-0 flex-1 flex-col">
            <h4 className="label-section mb-1">Previous (Parent)</h4>
            <pre className="flex-1 overflow-auto rounded-panel border border-line-inner bg-code p-2 font-mono text-[10px] text-ink-dim">
              {JSON.stringify(previousState, null, 2)}
            </pre>
          </div>
          <div className="flex min-h-0 flex-1 flex-col">
            <h4 className="label-section mb-1 text-iris-text">Current</h4>
            <pre className="flex-1 overflow-auto rounded-panel border border-line-inner bg-code p-2 font-mono text-[10px] text-ink-mid">
              {JSON.stringify(span.memory_state, null, 2)}
            </pre>
          </div>
        </div>
      ) : (
        <pre className="min-h-0 flex-1 overflow-auto rounded-panel border border-line-inner bg-code p-3 font-mono text-xs text-ink-mid">
          {JSON.stringify(span.memory_state, null, 2)}
        </pre>
      )}
    </div>
  );
}
