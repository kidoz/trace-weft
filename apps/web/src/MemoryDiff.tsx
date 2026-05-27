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
    <div className="mb-6 flex-1 flex flex-col min-h-0">
      <h3 className="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-2">
        Memory State (Scratchpad)
      </h3>
      {previousState ? (
        <div className="flex gap-2 flex-1 min-h-0">
          <div className="flex-1 flex flex-col min-h-0">
            <h4 className="text-xs font-bold text-slate-400 mb-1">Previous (Parent)</h4>
            <pre className="flex-1 bg-slate-100 border border-slate-200 text-slate-500 p-2 rounded text-[10px] overflow-auto">
              {JSON.stringify(previousState, null, 2)}
            </pre>
          </div>
          <div className="flex-1 flex flex-col min-h-0">
            <h4 className="text-xs font-bold text-blue-500 mb-1">Current</h4>
            <pre className="flex-1 bg-blue-50 border border-blue-100 text-blue-900 p-2 rounded text-[10px] overflow-auto">
              {JSON.stringify(span.memory_state, null, 2)}
            </pre>
          </div>
        </div>
      ) : (
        <pre className="bg-blue-50 border border-blue-100 text-blue-900 p-3 rounded text-xs overflow-auto flex-1 min-h-0">
          {JSON.stringify(span.memory_state, null, 2)}
        </pre>
      )}
    </div>
  );
}
