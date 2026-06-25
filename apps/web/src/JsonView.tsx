import { Fragment } from 'react';

// Token-colored JSON renderer for the Graphite inspector. Pretty-prints a value
// with JSON.stringify and colors keys / strings / numbers / null / punctuation
// per the design tokens — no external dependency.

type Token = { text: string; cls: string };

const COLORS = {
  key: 'text-[#7dd3fc]',
  string: 'text-jsonstr',
  number: 'text-warn',
  boolean: 'text-warn',
  null: 'text-error',
  punct: 'text-ink-dim',
} as const;

// Tokenize the already-formatted JSON string line so we can color it. We rely
// on JSON.stringify's stable formatting (2-space indent) and walk each line.
function tokenizeLine(line: string): Token[] {
  const tokens: Token[] = [];
  // Split into a leading "key": part and the remaining value part.
  const keyMatch = line.match(/^(\s*)("(?:\\.|[^"\\])*")(\s*:\s*)(.*)$/);
  if (keyMatch) {
    const [, indent, key, colon, rest] = keyMatch;
    if (indent) tokens.push({ text: indent, cls: COLORS.punct });
    tokens.push({ text: key, cls: COLORS.key });
    tokens.push({ text: colon, cls: COLORS.punct });
    tokens.push(...tokenizeValue(rest));
    return tokens;
  }
  return tokenizeValue(line);
}

function tokenizeValue(segment: string): Token[] {
  const tokens: Token[] = [];
  const indentMatch = segment.match(/^(\s*)(.*)$/);
  const indent = indentMatch?.[1] ?? '';
  const body = indentMatch?.[2] ?? segment;
  if (indent) tokens.push({ text: indent, cls: COLORS.punct });

  // Trailing comma is punctuation.
  let value = body;
  let trailing = '';
  if (value.endsWith(',')) {
    trailing = ',';
    value = value.slice(0, -1);
  }

  if (/^".*"$/.test(value)) {
    tokens.push({ text: value, cls: COLORS.string });
  } else if (value === 'null') {
    tokens.push({ text: value, cls: COLORS.null });
  } else if (value === 'true' || value === 'false') {
    tokens.push({ text: value, cls: COLORS.boolean });
  } else if (/^-?\d/.test(value)) {
    tokens.push({ text: value, cls: COLORS.number });
  } else {
    // Braces, brackets, empty objects, etc.
    tokens.push({ text: value, cls: COLORS.punct });
  }
  if (trailing) tokens.push({ text: trailing, cls: COLORS.punct });
  return tokens;
}

export function JsonView({ value, className = '' }: { value: unknown; className?: string }) {
  let text: string;
  try {
    // JSON.stringify is typed to return string, but at runtime returns undefined
    // for an undefined input; guard so the later .split never throws.
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
    text = JSON.stringify(value, null, 2) ?? 'undefined';
  } catch {
    text = `${value}`;
  }
  const lines = text.split('\n');

  return (
    <pre
      className={`overflow-auto rounded-panel border border-line-inner bg-code p-4 font-mono text-[11.5px] leading-[1.75] ${className}`}
    >
      <code>
        {lines.map((line, i) => (
          <Fragment key={i}>
            {tokenizeLine(line).map((t, j) => (
              <span key={j} className={t.cls}>
                {t.text}
              </span>
            ))}
            {i < lines.length - 1 ? '\n' : null}
          </Fragment>
        ))}
      </code>
    </pre>
  );
}
