import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { TraceList } from './TraceList';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count, estimateSize }: { count: number; estimateSize: () => number }) => {
    const size = estimateSize();
    return {
      getTotalSize: () => count * size,
      getVirtualItems: () =>
        Array.from({ length: count }, (_, index) => ({
          index,
          key: index,
          size,
          start: index * size,
        })),
    };
  },
}));

function renderWithQuery(ui: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('TraceList', () => {
  it('renders trace summaries from the typed API client', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () =>
            Promise.resolve([
              {
                trace_id: 'trace-1',
                run_id: 'run-1',
                start_time: 1715000000000,
                end_time: 1715000000100,
                span_count: 3,
                status: 'ok',
                root_name: 'support-agent',
                root_span_kind: 'agent',
                model_provider: 'openai',
                model_name: 'gpt-4.1',
                error_summary: null,
              },
            ]),
        }),
      ),
    );

    renderWithQuery(<TraceList onSelectTrace={() => {}} onDiffTraces={() => {}} />);

    await waitFor(() => expect(screen.getByText('support-agent')).toBeInTheDocument());
    expect(screen.getByText('openai / gpt-4.1')).toBeInTheDocument();
    expect(screen.getByText('3')).toBeInTheDocument();
  });
});
