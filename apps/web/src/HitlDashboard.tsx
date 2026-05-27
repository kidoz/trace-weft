import { useState, useEffect } from 'react';
import { CheckCircle2, ShieldAlert, XCircle } from 'lucide-react';

export function HitlDashboard() {
  const [pendingIds, setPendingIds] = useState<string[]>([]);
  const [selectedSpan, setSelectedSpan] = useState<string | null>(null);
  const [mockValue, setMockValue] = useState<string>('{}');
  const [rejectReason, setRejectReason] = useState<string>('Manually rejected in UI');

  useEffect(() => {
    const fetchPending = async () => {
      try {
        const res = await fetch('http://127.0.0.1:3000/api/hitl/pending');
        if (res.ok) {
          const ids = (await res.json()) as string[];
          setPendingIds(ids);
          if (ids.length > 0 && !selectedSpan) {
            setSelectedSpan(ids[0]);
          } else if (ids.length === 0) {
            setSelectedSpan(null);
          }
        }
      } catch (err) {
        console.error('Failed to fetch pending HITL approvals', err);
      }
    };

    fetchPending().catch(console.error);
    const interval = setInterval(() => {
      void fetchPending();
    }, 2000);
    return () => clearInterval(interval);
  }, [selectedSpan]);

  const handleResolve = async (action: 'approve' | 'reject') => {
    if (!selectedSpan) return;

    let value = null;
    if (action === 'approve') {
      try {
        value = JSON.parse(mockValue) as unknown;
      } catch {
        alert('Invalid JSON for approval value');
        return;
      }
    }

    try {
      await fetch('http://127.0.0.1:3000/api/hitl/resolve', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          span_id: selectedSpan,
          action,
          value,
          reason: rejectReason,
        }),
      });
      setSelectedSpan(null);
      setMockValue('{}');
    } catch (err) {
      console.error('Failed to resolve HITL', err);
    }
  };

  if (pendingIds.length === 0) return null;

  return (
    <div className="mx-6 mt-4 rounded border border-amber-200 bg-amber-50 p-4 shadow-sm">
      <div className="flex items-start">
        <div className="mr-3 rounded border border-amber-200 bg-white p-2 text-amber-700">
          <ShieldAlert className="h-5 w-5" aria-hidden="true" />
        </div>
        <div className="flex-1">
          <h3 className="mb-2 font-bold text-amber-900">
            Human-in-the-Loop: Pending Approvals ({pendingIds.length})
          </h3>
          <div className="flex gap-4">
            <div className="w-1/3 border-r border-yellow-200 pr-4">
              <ul className="space-y-1">
                {pendingIds.map((id) => (
                  <li
                    key={id}
                    onClick={() => setSelectedSpan(id)}
                    className={`cursor-pointer px-2 py-1 rounded text-sm font-mono ${
                      selectedSpan === id
                        ? 'bg-yellow-200 text-yellow-900 font-bold'
                        : 'text-yellow-700 hover:bg-yellow-100'
                    }`}
                  >
                    {id.substring(0, 13)}...
                  </li>
                ))}
              </ul>
            </div>
            {selectedSpan && (
              <div className="w-2/3 flex flex-col gap-3">
                <div>
                  <label className="block text-xs font-semibold uppercase text-yellow-800 mb-1">
                    Override Payload (JSON)
                  </label>
                  <textarea
                    className="w-full h-24 font-mono text-xs p-2 bg-yellow-100/50 text-slate-800 rounded border border-yellow-300 focus:outline-none focus:ring-2 focus:ring-yellow-500"
                    value={mockValue}
                    onChange={(e) => setMockValue(e.target.value)}
                  />
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={() => void handleResolve('approve')}
                    className="inline-flex items-center gap-1.5 rounded bg-emerald-600 px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-emerald-700"
                  >
                    <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
                    Approve
                  </button>
                  <div className="flex-1 flex gap-2">
                    <input
                      type="text"
                      className="flex-1 text-xs p-1.5 rounded border border-yellow-300 bg-white"
                      value={rejectReason}
                      onChange={(e) => setRejectReason(e.target.value)}
                    />
                    <button
                      onClick={() => void handleResolve('reject')}
                      className="inline-flex items-center gap-1.5 rounded bg-rose-600 px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-rose-700"
                    >
                      <XCircle className="h-4 w-4" aria-hidden="true" />
                      Reject
                    </button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
