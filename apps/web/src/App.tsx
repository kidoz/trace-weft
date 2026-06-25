import { useState } from 'react';
import { TraceList } from './TraceList';
import { TraceDetail } from './TraceDetail';
import { TraceDiff } from './TraceDiff';
import { HitlDashboard } from './HitlDashboard';
import { EvalDashboard } from './EvalDashboard';
import { TraceWeftMark } from './IconSystem';

type Tab = 'traces' | 'evals';

function App() {
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const [diffTraceIds, setDiffTraceIds] = useState<[string, string] | null>(null);
  const [activeTab, setActiveTab] = useState<Tab>('traces');

  const handleBack = () => {
    setSelectedTraceId(null);
    setDiffTraceIds(null);
  };

  const atRoot = !selectedTraceId && !diffTraceIds;

  const navTab = (tab: Tab, label: string) => {
    const active = activeTab === tab && atRoot;
    return (
      <button
        onClick={() => {
          setActiveTab(tab);
          handleBack();
        }}
        className={`rounded-pill px-3.5 py-1.5 text-[13px] font-semibold transition-colors ${
          active ? 'bg-iris text-window shadow-iris' : 'text-ink-mid hover:text-ink-hi'
        }`}
      >
        {label}
      </button>
    );
  };

  return (
    <div className="flex min-h-screen flex-col bg-window text-ink-hi">
      {/* Desktop title bar (Tauri). Harmless in the web build. */}
      <div data-tauri-drag-region className="flex h-[38px] shrink-0 items-center bg-titlebar px-4">
        <div className="flex items-center gap-2">
          <span className="h-3 w-3 rounded-full bg-[#ff5f57]" />
          <span className="h-3 w-3 rounded-full bg-[#febc2e]" />
          <span className="h-3 w-3 rounded-full bg-[#28c840]" />
        </div>
        <div className="flex-1 text-center text-xs font-medium text-ink-dim">TraceWeft</div>
        <div className="w-[52px]" />
      </div>

      {/* Top navigation */}
      <header className="sticky top-0 z-20 flex h-[60px] shrink-0 items-center justify-between border-b border-line-inner bg-nav px-6">
        <div className="flex items-center gap-2.5">
          <TraceWeftMark className="h-9 w-9" />
          <div className="leading-tight">
            <div className="text-[15px] font-bold text-ink-hi">TraceWeft</div>
            <div className="text-[9px] font-semibold uppercase tracking-[0.20em] text-iris">
              Local Trace Workbench
            </div>
          </div>
        </div>

        <nav className="flex items-center gap-1 rounded-[9px] border border-line bg-panel p-1">
          {navTab('traces', 'Runs')}
          {navTab('evals', 'Evaluations')}
        </nav>

        <div className="hidden items-center gap-2 rounded-pill border border-line bg-panel px-3 py-1.5 text-xs text-ink-mid md:flex">
          <span className="h-2 w-2 rounded-full bg-flow shadow-[0_0_8px_rgba(86,207,225,0.6)]" />
          <span className="font-mono">127.0.0.1:3000</span>
        </div>
      </header>

      <HitlDashboard />

      <main className="flex-1 overflow-hidden">
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
