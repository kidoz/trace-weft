import { useState } from 'react';
import { TraceList } from './TraceList';
import { TraceDetail } from './TraceDetail';
import { TraceDiff } from './TraceDiff';
import { HitlDashboard } from './HitlDashboard';
import { EvalDashboard } from './EvalDashboard';
import { navigationIcons } from './IconRegistry';
import { TraceWeftMark } from './IconSystem';

function App() {
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const [diffTraceIds, setDiffTraceIds] = useState<[string, string] | null>(null);
  const [activeTab, setActiveTab] = useState<'traces' | 'evals'>('traces');

  const handleBack = () => {
    setSelectedTraceId(null);
    setDiffTraceIds(null);
  };

  const RunsIcon = navigationIcons.runs;
  const EvalIcon = navigationIcons.dashboard;
  const TerminalIcon = navigationIcons.terminal;

  return (
    <div className="min-h-screen bg-slate-100 text-slate-950">
      <header className="sticky top-0 z-20 border-b border-slate-800 bg-slate-950 text-white shadow-sm">
        <div className="mx-auto flex h-16 max-w-7xl items-center justify-between px-6">
          <div className="flex items-center gap-2">
            <TraceWeftMark />
            <div>
              <div className="text-sm font-bold tracking-tight">TraceWeft</div>
              <div className="text-[11px] font-medium uppercase tracking-[0.18em] text-cyan-200">
                local trace workbench
              </div>
            </div>
          </div>

          <nav className="flex items-center gap-1 text-sm font-medium">
            <button
              className={`inline-flex items-center gap-2 rounded px-3 py-2 transition-colors ${
                activeTab === 'traces' && !selectedTraceId && !diffTraceIds
                  ? 'bg-white text-slate-950'
                  : 'text-slate-300 hover:bg-slate-800 hover:text-white'
              }`}
              onClick={() => {
                setActiveTab('traces');
                handleBack();
              }}
            >
              <RunsIcon className="h-4 w-4" aria-hidden="true" />
              Runs
            </button>
            <button
              className={`inline-flex items-center gap-2 rounded px-3 py-2 transition-colors ${
                activeTab === 'evals' && !selectedTraceId && !diffTraceIds
                  ? 'bg-white text-slate-950'
                  : 'text-slate-300 hover:bg-slate-800 hover:text-white'
              }`}
              onClick={() => {
                setActiveTab('evals');
                handleBack();
              }}
            >
              <EvalIcon className="h-4 w-4" aria-hidden="true" />
              Evaluations
            </button>
          </nav>

          <div className="hidden items-center gap-2 rounded border border-slate-700 bg-slate-900 px-3 py-1.5 text-xs text-slate-300 md:flex">
            <TerminalIcon className="h-4 w-4 text-cyan-300" aria-hidden="true" />
            <span className="font-mono">127.0.0.1:3000</span>
          </div>
        </div>
      </header>

      <HitlDashboard />

      <main>
        {diffTraceIds ? (
          <TraceDiff traceA={diffTraceIds[0]} traceB={diffTraceIds[1]} onBack={handleBack} />
        ) : selectedTraceId ? (
          <TraceDetail traceId={selectedTraceId} onBack={handleBack} />
        ) : activeTab === 'traces' ? (
          <TraceList
            onSelectTrace={setSelectedTraceId}
            onDiffTraces={(a, b) => setDiffTraceIds([a, b])}
          />
        ) : (
          <EvalDashboard onSelectTrace={setSelectedTraceId} />
        )}
      </main>
    </div>
  );
}

export default App;
