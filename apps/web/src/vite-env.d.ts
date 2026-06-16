/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Absolute base URL for the TraceWeft API; empty for same-origin `/api`. */
  readonly VITE_API_BASE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
