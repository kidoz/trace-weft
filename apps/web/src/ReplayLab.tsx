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
    <div className="p-8 max-w-4xl mx-auto">
      <div className="flex items-center mb-6">
        <button
          onClick={onBack}
          className="mr-4 inline-flex items-center gap-2 rounded border border-slate-200 bg-white px-3 py-2 text-sm font-medium text-slate-600 transition-colors hover:text-slate-950"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          Back
        </button>
        <h1 className="inline-flex items-center gap-2 text-xl font-bold text-slate-900">
          <Play className="h-5 w-5 text-lime-700" aria-hidden="true" />
          Replay Lab
        </h1>
      </div>

      <div className="rounded border border-slate-200 bg-white p-6 shadow-sm">
        <div className="mb-4">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-1">
            Target Span
          </h2>
          <div className="flex flex-wrap items-center gap-2 font-mono text-lg text-sky-700">
            <span>{span.name}</span>
            <SpanKindBadge kind={span.span_kind} />
          </div>
        </div>

        <div className="mb-6">
          <label className="block text-sm font-semibold uppercase tracking-wider text-slate-500 mb-2">
            Mocked Output (JSON)
          </label>
          <textarea
            className="w-full h-48 font-mono text-sm p-4 bg-slate-900 text-green-400 rounded-md shadow-inner focus:outline-none focus:ring-2 focus:ring-blue-500"
            value={mockContent}
            onChange={(e) => setMockContent(e.target.value)}
          />
          {error && <div className="text-red-500 text-sm mt-2">{error}</div>}
        </div>

        <div className="flex items-center justify-between bg-slate-50 p-4 rounded border border-slate-200">
          <div className="text-sm text-slate-600">
            <p>
              <strong>Instructions:</strong>
            </p>
            <ol className="list-decimal ml-4 mt-1">
              <li>Edit the JSON mock output above.</li>
              <li>Download the Replay Configuration file.</li>
              <li>
                Run your agent with: <br />
                <code className="bg-slate-200 px-1 py-0.5 rounded text-xs text-slate-800">
                  TRACE_WEFT_REPLAY_FILE=replay_config_{span.name}.json cargo run
                </code>
              </li>
            </ol>
          </div>
          <button
            onClick={handleDownload}
            className="inline-flex items-center gap-2 rounded bg-slate-950 px-6 py-3 font-medium text-white shadow-sm transition-colors hover:bg-slate-800"
          >
            <Download className="h-4 w-4" aria-hidden="true" />
            Download Config
          </button>
        </div>
      </div>
    </div>
  );
}
