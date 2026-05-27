import type { LucideIcon } from 'lucide-react';
import {
  Activity,
  Bot,
  BrainCircuit,
  Braces,
  CheckCircle2,
  CircleAlert,
  Clock3,
  Database,
  GitCompareArrows,
  MemoryStick,
  Play,
  Route,
  Search,
  ShieldCheck,
  Workflow,
  XCircle,
} from 'lucide-react';

const spanKindIcons: Record<string, LucideIcon> = {
  agent: Bot,
  checkpoint: CheckCircle2,
  embedding: BrainCircuit,
  evaluator: CheckCircle2,
  error: CircleAlert,
  guardrail: ShieldCheck,
  handoff: Route,
  llm_call: BrainCircuit,
  memory: MemoryStick,
  planner: Workflow,
  replay: Play,
  retrieval: Search,
  rerank: GitCompareArrows,
  router: Route,
  state: Database,
  tool: Braces,
  workflow: Workflow,
};

const spanKindStyles: Record<string, string> = {
  agent: 'bg-violet-50 text-violet-700 border-violet-200',
  checkpoint: 'bg-emerald-50 text-emerald-700 border-emerald-200',
  embedding: 'bg-cyan-50 text-cyan-700 border-cyan-200',
  evaluator: 'bg-emerald-50 text-emerald-700 border-emerald-200',
  error: 'bg-rose-50 text-rose-700 border-rose-200',
  guardrail: 'bg-teal-50 text-teal-700 border-teal-200',
  handoff: 'bg-indigo-50 text-indigo-700 border-indigo-200',
  llm_call: 'bg-sky-50 text-sky-700 border-sky-200',
  memory: 'bg-fuchsia-50 text-fuchsia-700 border-fuchsia-200',
  planner: 'bg-amber-50 text-amber-700 border-amber-200',
  replay: 'bg-lime-50 text-lime-700 border-lime-200',
  retrieval: 'bg-blue-50 text-blue-700 border-blue-200',
  rerank: 'bg-orange-50 text-orange-700 border-orange-200',
  router: 'bg-indigo-50 text-indigo-700 border-indigo-200',
  state: 'bg-slate-50 text-slate-700 border-slate-200',
  tool: 'bg-orange-50 text-orange-700 border-orange-200',
  workflow: 'bg-slate-100 text-slate-700 border-slate-300',
};

export function TraceWeftMark({ className = 'h-8 w-8' }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 48 48" role="img" aria-label="TraceWeft">
      <rect width="48" height="48" rx="8" fill="#0f172a" />
      <path
        d="M12 16.5h7.5c3.4 0 5.4 2 8 7.5s4.6 7.5 8 7.5H36"
        fill="none"
        stroke="#67e8f9"
        strokeLinecap="round"
        strokeWidth="4"
      />
      <path
        d="M12 31.5h7.5c3.4 0 5.4-2 8-7.5s4.6-7.5 8-7.5H36"
        fill="none"
        stroke="#f59e0b"
        strokeLinecap="round"
        strokeWidth="4"
      />
      <circle cx="12" cy="16.5" r="3" fill="#e2e8f0" />
      <circle cx="12" cy="31.5" r="3" fill="#e2e8f0" />
      <circle cx="36" cy="16.5" r="3" fill="#e2e8f0" />
      <circle cx="36" cy="31.5" r="3" fill="#e2e8f0" />
    </svg>
  );
}

export function SpanKindIcon({
  kind,
  className = 'h-4 w-4',
}: {
  kind: string;
  className?: string;
}) {
  const Icon = spanKindIcons[kind.toLowerCase()] ?? Activity;
  return <Icon className={className} aria-hidden="true" strokeWidth={2} />;
}

export function SpanKindBadge({ kind }: { kind: string }) {
  const normalizedKind = kind.toLowerCase();
  const style = spanKindStyles[normalizedKind] ?? spanKindStyles.workflow;

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded border px-2 py-0.5 text-[11px] font-semibold uppercase ${style}`}
    >
      <SpanKindIcon kind={normalizedKind} className="h-3.5 w-3.5" />
      {kind}
    </span>
  );
}

export function StatusBadge({ status }: { status: string }) {
  const ok = status === 'ok' || status === 'pass';
  const Icon = ok ? CheckCircle2 : XCircle;

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded border px-2 py-0.5 text-xs font-semibold uppercase ${
        ok
          ? 'border-emerald-200 bg-emerald-50 text-emerald-700'
          : 'border-rose-200 bg-rose-50 text-rose-700'
      }`}
    >
      <Icon className="h-3.5 w-3.5" aria-hidden="true" />
      {status}
    </span>
  );
}

export function MetricPill({ icon: Icon = Clock3, label }: { icon?: LucideIcon; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded border border-slate-200 bg-white px-2 py-1 text-xs font-medium text-slate-600">
      <Icon className="h-3.5 w-3.5 text-slate-400" aria-hidden="true" />
      {label}
    </span>
  );
}
