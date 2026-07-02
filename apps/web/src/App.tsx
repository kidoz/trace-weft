import { useState } from 'react';
import { TraceList } from './TraceList';
import { TraceDetail } from './TraceDetail';
import { TraceDiff } from './TraceDiff';
import { HitlDashboard } from './HitlDashboard';
import { EvalDashboard } from './EvalDashboard';
import { TraceWeftMark } from './IconSystem';
import { ApiKeyField } from './ApiKeyField';

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

        <ApiKeyField />
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
