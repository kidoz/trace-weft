import type { Span } from './TraceDetail';

export function TokenHeatmap({ tokenUsage }: { tokenUsage: Span['token_usage'] }) {
  if (!tokenUsage) return null;

  const total = tokenUsage.input + tokenUsage.output;
  if (total === 0) return null;

  const inputPct = (tokenUsage.input / total) * 100;
  const outputPct = (tokenUsage.output / total) * 100;

  // Render breakdowns if available
  const hasBreakdown = Object.keys(tokenUsage.breakdown).length > 0;

  return (
    <div className="mb-6">
      <h3 className="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-2">
        Token Heatmap
      </h3>
      <div className="bg-slate-50 border border-slate-200 rounded p-4">
        <div className="flex justify-between text-xs text-slate-600 mb-2">
          <span>Input: {tokenUsage.input}</span>
          <span>Output: {tokenUsage.output}</span>
          <span className="font-bold">Total: {total}</span>
        </div>

        {/* Simple Progress Bar */}
        <div className="w-full h-4 bg-slate-200 rounded overflow-hidden flex">
          <div
            style={{ width: `${inputPct}%` }}
            className="bg-blue-500 h-full"
            title="Input Tokens"
          ></div>
          <div
            style={{ width: `${outputPct}%` }}
            className="bg-green-500 h-full"
            title="Output Tokens"
          ></div>
        </div>

        {hasBreakdown && (
          <div className="mt-4 border-t border-slate-200 pt-3">
            <h4 className="text-xs font-bold text-slate-500 mb-2">Breakdown</h4>
            <div className="grid grid-cols-2 gap-2 text-xs">
              {Object.entries(tokenUsage.breakdown).map(([key, val]) => (
                <div
                  key={key}
                  className="flex justify-between bg-white p-1.5 rounded border border-slate-100"
                >
                  <span className="text-slate-600">{key}</span>
                  <span className="font-mono text-slate-800">{val}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
