import type { Span } from './api';

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
      <h3 className="label-section mb-2">Token Heatmap</h3>
      <div className="rounded-panel border border-line bg-panel p-4">
        <div className="mb-2 flex justify-between font-mono text-xs text-ink-mid">
          <span>
            Input: <span className="text-flow">{tokenUsage.input}</span>
          </span>
          <span>
            Output: <span className="text-ok">{tokenUsage.output}</span>
          </span>
          <span className="font-bold text-ink-hi">Total: {total}</span>
        </div>

        {/* Simple Progress Bar */}
        <div className="flex h-4 w-full overflow-hidden rounded-chip bg-panel-2 ring-1 ring-inset ring-line-inner">
          <div
            style={{ width: `${inputPct}%`, backgroundColor: 'rgba(86, 207, 225, 0.65)' }}
            className="h-full"
            title="Input Tokens"
          ></div>
          <div
            style={{ width: `${outputPct}%`, backgroundColor: 'rgba(74, 222, 128, 0.65)' }}
            className="h-full"
            title="Output Tokens"
          ></div>
        </div>

        {hasBreakdown && (
          <div className="mt-4 border-t border-line-inner pt-3">
            <h4 className="label-section mb-2">Breakdown</h4>
            <div className="grid grid-cols-2 gap-2 text-xs">
              {Object.entries(tokenUsage.breakdown).map(([key, val]) => (
                <div
                  key={key}
                  className="flex justify-between rounded-chip border border-line-inner bg-panel-2 p-1.5"
                >
                  <span className="text-ink-mid">{key}</span>
                  <span className="font-mono text-ink-hi">{val}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
