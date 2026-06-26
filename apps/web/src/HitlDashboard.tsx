import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, Pause, X } from 'lucide-react';
import { api, queryKeys } from './api';

export function HitlDashboard() {
  const [selectedSpan, setSelectedSpan] = useState<string | null>(null);
  const [mockValue, setMockValue] = useState<string>('{}');
  const [rejectReason, setRejectReason] = useState<string>('Manually rejected in UI');
  const queryClient = useQueryClient();

  const { data: pendingIds = [] } = useQuery({
    queryKey: queryKeys.hitlPending,
    queryFn: api.getPendingApprovals,
    refetchInterval: 2000,
  });
  const activeSpan =
    selectedSpan && pendingIds.includes(selectedSpan) ? selectedSpan : (pendingIds[0] ?? null);

  const resolveMutation = useMutation({
    mutationFn: api.resolveApproval,
    onSuccess: async () => {
      setSelectedSpan(null);
      setMockValue('{}');
      await queryClient.invalidateQueries({ queryKey: queryKeys.hitlPending });
    },
  });

  const handleResolve = (action: 'approve' | 'reject') => {
    if (!activeSpan) return;

    let value = null;
    if (action === 'approve') {
      try {
        value = JSON.parse(mockValue) as unknown;
      } catch {
        alert('Invalid JSON for approval value');
        return;
      }
    }

    resolveMutation.mutate({
      span_id: activeSpan,
      action,
      value,
      reason: rejectReason,
    });
  };

  if (pendingIds.length === 0) return null;

  return (
    <div className="mx-6 mt-4">
      <div className="overflow-hidden rounded-window border border-[rgba(251,191,36,0.3)] bg-nav">
        {/* Header bar */}
        <div className="flex items-center gap-3 bg-[linear-gradient(90deg,rgba(251,191,36,0.12),rgba(251,191,36,0.02))] p-4">
          <div className="rounded-panel bg-[rgba(251,191,36,0.15)] p-2 text-warn">
            <Pause className="h-5 w-5" aria-hidden="true" />
          </div>
          <div className="flex-1">
            <div className="font-semibold text-warn-text">Breakpoint · Human-in-the-Loop</div>
            <div className="text-xs text-ink-mid">
              Execution paused — {pendingIds.length} span(s) awaiting approval
            </div>
          </div>
          <span className="rounded-pill border border-[rgba(251,191,36,0.3)] bg-[rgba(251,191,36,0.12)] px-2.5 py-1 font-mono text-xs text-warn">
            {pendingIds.length} PENDING
          </span>
        </div>

        {/* Body */}
        <div className="grid grid-cols-[300px_1fr] border-t border-line-inner">
          {/* Queue */}
          <div className="border-r border-line-inner p-4">
            <div className="label-section">QUEUE</div>
            <div className="mt-2 space-y-2">
              {pendingIds.map((id) => {
                const active = activeSpan === id;
                return (
                  <div
                    key={id}
                    onClick={() => setSelectedSpan(id)}
                    className={`cursor-pointer rounded-panel p-2.5 transition-colors ${
                      active
                        ? 'border border-[rgba(251,191,36,0.3)] bg-[rgba(251,191,36,0.10)]'
                        : 'border border-line-inner bg-panel'
                    }`}
                  >
                    <div
                      className={`truncate font-mono text-xs ${
                        active ? 'text-ink-hi' : 'text-ink-mid'
                      }`}
                    >
                      {id}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Detail */}
          {activeSpan && (
            <div className="p-5">
              <div className="flex items-center gap-3">
                <div className="flex-1">
                  <div className="font-mono text-ink-hi">{activeSpan}</div>
                  <div className="text-xs text-ink-dim">tool · awaiting approval</div>
                </div>
                <span className="rounded-chip border border-[rgba(251,191,36,0.25)] bg-[rgba(251,191,36,0.10)] px-2 py-1 text-xs text-warn">
                  PENDING APPROVAL
                </span>
              </div>

              <div className="mt-4">
                <div className="label-section">Override payload (JSON)</div>
                <textarea
                  className="mt-2 h-24 w-full rounded-panel border border-line-input bg-surface p-3 font-mono text-xs text-ink-hi caret-[#7c83ff] focus:border-iris focus:outline-none"
                  value={mockValue}
                  onChange={(e) => setMockValue(e.target.value)}
                />
              </div>

              <div className="mt-4 flex items-center gap-2">
                <button
                  onClick={() => handleResolve('approve')}
                  className="inline-flex items-center gap-1.5 rounded-pill bg-[#22c55e] px-4 py-1.5 font-semibold text-window shadow-[0_4px_14px_rgba(34,197,94,0.30)] transition-colors"
                >
                  <Check className="h-4 w-4" aria-hidden="true" />
                  Approve &amp; resume
                </button>
                <input
                  type="text"
                  className="flex-1 rounded-pill border border-line-inner bg-surface px-3 py-1.5 font-mono text-xs text-ink-hi focus:outline-none"
                  value={rejectReason}
                  onChange={(e) => setRejectReason(e.target.value)}
                />
                <button
                  onClick={() => handleResolve('reject')}
                  className="inline-flex items-center gap-1.5 rounded-pill border border-[rgba(251,113,133,0.4)] bg-nav px-4 py-1.5 font-semibold text-error transition-colors"
                >
                  <X className="h-4 w-4" aria-hidden="true" />
                  Reject
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
