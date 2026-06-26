import { useMemo } from 'react';
import {
  ReactFlow,
  MiniMap,
  Controls,
  Background,
  BackgroundVariant,
  MarkerType,
  type Node,
  type Edge,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import dagre from 'dagre';
import type { Span } from './TraceDetail';
import { SpanKindBadge } from './IconSystem';
import { spanKindColor } from './spanColors';

const nodeWidth = 210;
const nodeHeight = 84;

const getLayoutedElements = (nodes: Node[], edges: Edge[], direction = 'TB') => {
  // Build a fresh graph per layout. A module-level singleton would accumulate
  // every previously-viewed trace's nodes/edges and skew the layout of the
  // current trace (positions drift the longer the session runs).
  const dagreGraph = new dagre.graphlib.Graph();
  dagreGraph.setDefaultEdgeLabel(() => ({}));
  dagreGraph.setGraph({ rankdir: direction, ranksep: 70, nodesep: 36 });

  nodes.forEach((node) => {
    dagreGraph.setNode(node.id, { width: nodeWidth, height: nodeHeight });
  });
  edges.forEach((edge) => {
    dagreGraph.setEdge(edge.source, edge.target);
  });

  dagre.layout(dagreGraph);

  const newNodes = nodes.map((node) => {
    const nodeWithPosition = dagreGraph.node(node.id);
    return {
      ...node,
      targetPosition: direction === 'TB' ? 'top' : 'left',
      sourcePosition: direction === 'TB' ? 'bottom' : 'right',
      position: {
        x: nodeWithPosition.x - nodeWidth / 2,
        y: nodeWithPosition.y - nodeHeight / 2,
      },
    } as Node;
  });

  return { nodes: newNodes, edges };
};

function isPending(status: string): boolean {
  const s = status.toLowerCase();
  return s === 'pending' || s === 'waiting' || s === 'pending_approval';
}

export function TraceGraph({
  spans,
  onSpanClick,
}: {
  spans: Span[];
  onSpanClick: (span: Span) => void;
}) {
  const { nodes, edges } = useMemo(() => {
    const initialNodes: Node[] = spans.map((span) => {
      const error = span.status.toLowerCase() !== 'ok' && !isPending(span.status);
      const pending = isPending(span.status);
      const dur = span.latency_ms ?? (span.end_time ? span.end_time - span.start_time : 0);
      const dotColor = error ? '#fb7185' : pending ? '#fbbf24' : '#4ade80';

      return {
        id: span.span_id,
        position: { x: 0, y: 0 }, // computed by dagre
        data: {
          label: (
            <div
              onClick={() => onSpanClick(span)}
              className="w-[190px] rounded-[11px] border bg-panel p-2.5 text-left transition-colors hover:border-[rgba(124,131,255,0.5)]"
              style={{ borderColor: error ? 'rgba(251,113,133,0.5)' : '#262b34' }}
            >
              <div className="mb-1.5 flex items-center justify-between gap-2">
                <span className="truncate text-[12px] font-semibold text-ink-hi">{span.name}</span>
                <span
                  className={`h-2 w-2 shrink-0 rounded-full ${pending ? 'tw-pulse' : ''}`}
                  style={{ backgroundColor: dotColor }}
                />
              </div>
              <div className="flex items-center justify-between gap-2">
                <SpanKindBadge kind={span.span_kind} />
                <span className="font-mono text-[11px] text-ink-mid">{dur ? `${dur}ms` : '—'}</span>
              </div>
            </div>
          ),
        },
        style: { width: nodeWidth, padding: 0, border: 'none', background: 'transparent' },
      };
    });

    const initialEdges: Edge[] = spans
      .filter((span) => span.parent_span_id)
      .map((span) => ({
        id: `e-${span.parent_span_id}-${span.span_id}`,
        source: span.parent_span_id as string,
        target: span.span_id,
        type: 'smoothstep',
        markerEnd: {
          type: MarkerType.ArrowClosed,
          width: 18,
          height: 18,
          color: '#2a3040',
        },
        style: { strokeWidth: 1.6, stroke: '#2a3040' },
      }));

    return getLayoutedElements(initialNodes, initialEdges);
  }, [spans, onSpanClick]);

  const kindColorById = useMemo(() => {
    const m = new Map<string, string>();
    spans.forEach((s) => m.set(s.span_id, spanKindColor(s.span_kind)));
    return m;
  }, [spans]);

  return (
    <div className="h-full w-full bg-surface">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        colorMode="dark"
        fitView
        minZoom={0.1}
        proOptions={{ hideAttribution: true }}
      >
        <Controls className="!border-line-node !bg-panel" showInteractive={false} />
        <MiniMap
          pannable
          zoomable
          maskColor="rgba(8,10,14,0.7)"
          style={{ background: '#0a0c10', border: '1px solid #262b34', borderRadius: 8 }}
          nodeColor={(n) => kindColorById.get(n.id) ?? '#5d6677'}
          nodeStrokeWidth={0}
        />
        <Background
          variant={BackgroundVariant.Dots}
          gap={24}
          size={1.5}
          color="rgba(124,131,255,0.14)"
        />
      </ReactFlow>
    </div>
  );
}
