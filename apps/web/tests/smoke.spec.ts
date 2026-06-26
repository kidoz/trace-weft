import { expect, test } from '@playwright/test';

const traceId = '11111111-1111-7111-8111-111111111111';
const spanId = '22222222-2222-7222-8222-222222222222';

test.beforeEach(async ({ page }) => {
  await page.route('**/api/traces', async (route) => {
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify([
        {
          trace_id: traceId,
          run_id: 'run-1',
          start_time: 1715000000000,
          end_time: 1715000000500,
          span_count: 1,
          status: 'ok',
          root_name: 'support-agent',
          root_span_kind: 'agent',
          model_provider: 'openai',
          model_name: 'gpt-4.1',
          error_summary: null,
        },
      ]),
    });
  });
  await page.route(`**/api/traces/${traceId}`, async (route) => {
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify([
        {
          trace_id: traceId,
          span_id: spanId,
          parent_span_id: null,
          run_id: 'run-1',
          span_kind: 'agent',
          name: 'support-agent',
          start_time: 1715000000000,
          end_time: 1715000000500,
          status: 'ok',
          attributes: { temperature: 0.2 },
          otel_attributes: {},
          openinference_attributes: {},
          latency_ms: 500,
          input_ref: null,
          output_ref: null,
          retrieved_document_refs: [],
          token_usage: { input: 10, output: 5, reasoning: null, breakdown: {} },
          cost_estimate: { currency: 'USD', amount: 0.01 },
          redaction_policy: 'redacted_preview',
          schema_version: '1.0',
        },
      ]),
    });
  });
  await page.route(`**/api/traces/${traceId}/events`, async (route) => {
    await route.fulfill({ contentType: 'application/json', body: '[]' });
  });
  await page.route('**/api/hitl/pending', async (route) => {
    await route.fulfill({ contentType: 'application/json', body: '[]' });
  });
  await page.route(`**/api/traces/${traceId}/replay-plan/${spanId}`, async (route) => {
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        trace_id: traceId,
        target_span: {},
        config_template: { mocked_spans: {}, mocked_span_ids: {}, block_side_effects: true },
        command: 'TRACE_WEFT_REPLAY_FILE=replay_config_support-agent.json cargo run',
      }),
    });
  });
});

test('opens trace detail and replay lab', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('support-agent')).toBeVisible();
  await page.getByText('support-agent').click();
  await expect(page.getByText('Trace Detail')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Waterfall' })).toBeVisible();
  await page.getByRole('button', { name: /Mock Span/i }).click();
  await expect(page.getByText('Replay Lab')).toBeVisible();
});
