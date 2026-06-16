// Base URL for the TraceWeft API.
//
// Empty by default so the app uses same-origin relative `/api/...` paths: the
// Vite dev server proxies those to the API (see `vite.config.ts`), and a
// same-origin deployment serves them directly. The desktop build sets
// `VITE_API_BASE` to the embedded server's absolute origin
// (`http://127.0.0.1:3000`) since its webview is not same-origin with the API.
export const API_BASE = import.meta.env.VITE_API_BASE ?? '';

/** Resolve an API path (e.g. `/api/traces`) against the configured base. */
export const apiUrl = (path: string): string => `${API_BASE}${path}`;
