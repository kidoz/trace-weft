import { useState } from 'react';
import { ArrowLeft, Download, Play } from 'lucide-react';
import type { Span } from './TraceDetail';
import { SpanKindBadge } from './IconSystem';

export function ReplayLab({ span, onBack }: { span: Span; onBack: () => void }) {
  // Try to prefill with whatever output we can find, or just empty quotes if string
  const [mockContent, setMockContent] = useState<string>(() => {
    if (span.output_ref) {
      return JSON.stringify(span.output_ref, null, 2);
    }
    return '"mocked output"';
  });

  const [error, setError] = useState<string | null>(null);

  const handleDownload = () => {
    try {
      // Validate JSON
      const parsedValue = JSON.parse(mockContent);

      const config = {
        mocked_spans: {
          [span.name]: parsedValue,
        },
        block_side_effects: true,
      };

      const dataStr =
        'data:text/json;charset=utf-8,' + encodeURIComponent(JSON.stringify(config, null, 2));
      const dlAnchorElem = document.createElement('a');
      dlAnchorElem.setAttribute('href', dataStr);
      dlAnchorElem.setAttribute('download', `replay_config_${span.name}.json`);
      dlAnchorElem.click();
      setError(null);
    } catch (e: unknown) {
      if (e instanceof Error) {
        setError(`Invalid JSON: ${e.message}`);
      } else {
        setError(`Invalid JSON`);
      }
    }
  };

  return (
    <div className="mx-auto max-w-4xl p-8">
      <div className="mb-6 flex items-center">
        <button
          onClick={onBack}
          className="mr-4 inline-flex items-center gap-2 rounded-pill border border-line bg-panel px-3 py-2 text-sm font-medium text-ink-mid transition-colors hover:text-ink-hi"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          Back
        </button>
        <h1 className="inline-flex items-center gap-2 text-xl font-bold text-ink-hi">
          <Play className="h-5 w-5 text-iris" aria-hidden="true" />
          Replay Lab
        </h1>
      </div>

      <div className="rounded-window border border-line bg-surface p-6 shadow-window">
        <div className="mb-4">
          <h2 className="label-section mb-1">Target Span</h2>
          <div className="flex flex-wrap items-center gap-2 font-mono text-lg text-flow">
            <span>{span.name}</span>
            <SpanKindBadge kind={span.span_kind} />
          </div>
        </div>

        <div className="mb-6">
          <label className="label-section mb-2 block">Mocked Output (JSON)</label>
          <textarea
            className="h-48 w-full rounded-panel border border-line-inner bg-code p-4 font-mono text-sm text-jsonstr shadow-inner focus:outline-none focus:ring-2 focus:ring-iris"
            value={mockContent}
            onChange={(e) => setMockContent(e.target.value)}
          />
          {error && <div className="mt-2 text-sm text-error">{error}</div>}
        </div>

        <div className="flex items-center justify-between rounded-panel border border-line bg-panel p-4">
          <div className="text-sm text-ink-mid">
            <p>
              <strong className="text-ink-hi">Instructions:</strong>
            </p>
            <ol className="ml-4 mt-1 list-decimal">
              <li>Edit the JSON mock output above.</li>
              <li>Download the Replay Configuration file.</li>
              <li>
                Run your agent with: <br />
                <code className="rounded-chip bg-code px-1 py-0.5 font-mono text-xs text-flow">
                  TRACE_WEFT_REPLAY_FILE=replay_config_{span.name}.json cargo run
                </code>
              </li>
            </ol>
          </div>
          <button
            onClick={handleDownload}
            className="inline-flex items-center gap-2 rounded-pill bg-iris px-6 py-3 font-semibold text-window shadow-iris transition-[filter] hover:brightness-110"
          >
            <Download className="h-4 w-4" aria-hidden="true" />
            Download Config
          </button>
        </div>
      </div>
    </div>
  );
}
