// API-key storage for talking to an authenticated TraceWeft server.
//
// The server accepts `Authorization: Bearer <key>` and scopes queries to the
// key's project (see crates/trace-weft-server/src/auth.rs). When no key is set
// the UI sends no header, which works against a local-first dev-bypass server
// exactly as before. The key is kept in localStorage so it survives reloads;
// this is acceptable for a local workbench — do not point it at a shared
// untrusted machine.

const STORAGE_KEY = 'tw_api_key';

let apiKey: string | null =
  typeof localStorage !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null;

/** The currently configured API key, or `null` for the dev-bypass path. */
export function getApiKey(): string | null {
  return apiKey;
}

/** Persist (or clear, when empty) the API key. */
export function setApiKey(key: string | null): void {
  const trimmed = key?.trim() ?? '';
  apiKey = trimmed.length > 0 ? trimmed : null;
  if (typeof localStorage === 'undefined') return;
  if (apiKey) localStorage.setItem(STORAGE_KEY, apiKey);
  else localStorage.removeItem(STORAGE_KEY);
}

/** Auth headers to merge into a fetch — empty when no key is configured. */
export function authHeaders(): Record<string, string> {
  return apiKey ? { Authorization: `Bearer ${apiKey}` } : {};
}
