import { useMemo } from 'react';
import {
  ReactFlow,
  MiniMap,
  Controls,
  Background,
  MarkerType,
  type Node,
  type Edge,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import dagre from 'dagre';
import type { Span } from './TraceDetail';

const dagreGraph = new dagre.graphlib.Graph();
dagreGraph.setDefaultEdgeLabel(() => ({}));

const nodeWidth = 250;
const nodeHeight = 80;

const getLayoutedElements = (nodes: Node[], edges: Edge[], direction = 'TB') => {
  dagreGraph.setGraph({ rankdir: direction });

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

export function TraceGraph({
  spans,
  onSpanClick,
}: {
  spans: Span[];
  onSpanClick: (span: Span) => void;
}) {
  const { nodes, edges } = useMemo(() => {
    const initialNodes: Node[] = spans.map((span) => {
      const isError = span.status !== 'ok';
      let bgColor = 'bg-white';
      if (span.span_kind === 'agent') bgColor = 'bg-purple-50';
      else if (span.span_kind === 'llm_call') bgColor = 'bg-blue-50';
      else if (span.span_kind === 'tool') bgColor = 'bg-orange-50';

      return {
        id: span.span_id,
        position: { x: 0, y: 0 }, // computed by dagre
        data: {
          label: (
            <div
              className={`p-2 rounded border shadow-sm ${bgColor} ${isError ? 'border-red-400' : 'border-slate-200'} text-left`}
              onClick={() => onSpanClick(span)}
            >
              <div className="font-semibold text-slate-800 text-xs mb-1 truncate">{span.name}</div>
              <div className="flex justify-between text-[10px] text-slate-500">
                <span className="uppercase font-bold">{span.span_kind}</span>
                <span className={isError ? 'text-red-600' : ''}>{span.latency_ms}ms</span>
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
          width: 20,
          height: 20,
          color: '#cbd5e1',
        },
        style: {
          strokeWidth: 2,
          stroke: '#cbd5e1',
        },
      }));

    return getLayoutedElements(initialNodes, initialEdges);
  }, [spans, onSpanClick]);

  return (
    <div className="w-full h-full bg-slate-50/50">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        fitView
        minZoom={0.1}
        attributionPosition="bottom-right"
      >
        <Controls />
        <MiniMap />
        <Background gap={12} size={1} />
      </ReactFlow>
    </div>
  );
}
