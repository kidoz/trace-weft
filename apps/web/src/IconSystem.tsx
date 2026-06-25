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
import { spanKindColor, spanKindRgb } from './spanColors';

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

export function TraceWeftMark({ className = 'h-8 w-8' }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 48 48" role="img" aria-label="TraceWeft">
      <rect width="48" height="48" rx="10" fill="#14171d" />
      <path
        d="M12 16.5h7.5c3.4 0 5.4 2 8 7.5s4.6 7.5 8 7.5H36"
        fill="none"
        stroke="#7c83ff"
        strokeLinecap="round"
        strokeWidth="4"
      />
      <path
        d="M12 31.5h7.5c3.4 0 5.4-2 8-7.5s4.6-7.5 8-7.5H36"
        fill="none"
        stroke="#56cfe1"
        strokeLinecap="round"
        strokeWidth="4"
      />
      <circle cx="12" cy="16.5" r="3" fill="#cdd3e0" />
      <circle cx="12" cy="31.5" r="3" fill="#cdd3e0" />
      <circle cx="36" cy="16.5" r="3" fill="#cdd3e0" />
      <circle cx="36" cy="31.5" r="3" fill="#cdd3e0" />
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
  const hex = spanKindColor(kind);
  const rgb = spanKindRgb(kind);
  return (
    <span
      className="inline-flex items-center gap-1 rounded-chip border px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wide"
      style={{
        color: hex,
        backgroundColor: `rgba(${rgb}, 0.12)`,
        borderColor: `rgba(${rgb}, 0.30)`,
      }}
    >
      <SpanKindIcon kind={kind} className="h-3 w-3" />
      {kind}
    </span>
  );
}

type StatusTone = { hex: string; rgb: string };

function statusTone(status: string): StatusTone {
  const s = status.toLowerCase();
  if (s === 'ok' || s === 'pass') return { hex: '#4ade80', rgb: '74, 222, 128' };
  if (s === 'pending' || s === 'waiting' || s === 'pending_approval')
    return { hex: '#fbbf24', rgb: '251, 191, 36' };
  return { hex: '#fb7185', rgb: '251, 113, 133' };
}

export function StatusBadge({ status }: { status: string }) {
  const s = status.toLowerCase();
  const ok = s === 'ok' || s === 'pass';
  const pending = s === 'pending' || s === 'waiting' || s === 'pending_approval';
  const Icon = ok ? CheckCircle2 : pending ? Clock3 : XCircle;
  const tone = statusTone(status);
  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-chip border px-2 py-0.5 text-[10.5px] font-bold uppercase tracking-wide"
      style={{
        color: tone.hex,
        backgroundColor: `rgba(${tone.rgb}, 0.10)`,
        borderColor: `rgba(${tone.rgb}, 0.25)`,
      }}
    >
      <Icon className="h-3 w-3" aria-hidden="true" />
      {status}
    </span>
  );
}

export function MetricPill({ icon: Icon = Clock3, label }: { icon?: LucideIcon; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-pill border border-line bg-panel px-2.5 py-1 text-xs font-medium text-ink-mid">
      <Icon className="h-3.5 w-3.5 text-ink-dim" aria-hidden="true" />
      {label}
    </span>
  );
}
