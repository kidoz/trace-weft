// Graphite span-kind chip palette: each kind maps to a text hex; chip
// backgrounds/borders are derived as low-alpha tints of that hex. Kept in its
// own module so it can be shared by components without tripping the
// react-refresh "only export components" rule.
const spanKindColors: Record<string, string> = {
  agent: '#c4b5fd',
  llm_call: '#7dd3fc',
  embedding: '#7dd3fc',
  tool: '#fbbf24',
  state: '#fbbf24',
  retrieval: '#7dd3fc',
  rerank: '#fb923c',
  guardrail: '#5eead4',
  handoff: '#a5b4fc',
  router: '#a5b4fc',
  checkpoint: '#4ade80',
  evaluator: '#4ade80',
  memory: '#f0abfc',
  planner: '#a5b4fc',
  replay: '#7dd3fc',
  workflow: '#98a1b0',
  error: '#fb7185',
};

export function spanKindColor(kind: string): string {
  return spanKindColors[kind.toLowerCase()] ?? '#98a1b0';
}

export function spanKindRgb(kind: string): string {
  const hex = spanKindColor(kind).replace('#', '');
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  return `${r}, ${g}, ${b}`;
}
