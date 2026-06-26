import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { KeyRound } from 'lucide-react';
import { getApiKey, setApiKey } from './auth';

// Header control for entering an API key when talking to an authenticated
// server. Leaving it empty uses the local dev bypass (no Authorization header).
// On save we invalidate every query so data refetches under the new identity.
export function ApiKeyField() {
  const queryClient = useQueryClient();
  const [value, setValue] = useState(getApiKey() ?? '');
  const [saved, setSaved] = useState(false);

  const apply = () => {
    setApiKey(value);
    void queryClient.invalidateQueries();
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  };

  const dirty = value !== (getApiKey() ?? '');

  return (
    <div className="hidden items-center gap-2 rounded-pill border border-line bg-panel px-3 py-1.5 text-xs text-ink-mid md:flex">
      <KeyRound className="h-3.5 w-3.5 text-ink-dim" aria-hidden="true" />
      <input
        type="password"
        value={value}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') apply();
        }}
        placeholder="API key — empty = dev bypass"
        aria-label="API key"
        className="w-[170px] bg-transparent font-mono text-xs text-ink-hi outline-none placeholder:text-ink-faint"
      />
      <button
        onClick={apply}
        disabled={!dirty && !saved}
        className="font-semibold text-iris transition-colors hover:text-ink-hi disabled:text-ink-faint"
      >
        {saved ? 'Saved' : 'Set'}
      </button>
    </div>
  );
}
