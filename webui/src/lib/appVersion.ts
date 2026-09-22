/** Strip optional `v` prefix so `v7.0.13` and `7.0.13` compare/display the same. */
export function normalizeAppVersion(raw: string | undefined | null): string {
  return String(raw ?? '').trim().replace(/^v/i, '');
}

/** Sidebar brand label (`v7.0.13`). Empty input stays empty. */
export function formatAppVersionLabel(raw: string | undefined | null): string {
  const v = normalizeAppVersion(raw);
  return v ? `v${v}` : '';
}

/** Vite `define` fallback baked into the SPA. Empty when the constant is missing (tests). */
export function bakedAppVersion(): string {
  try {
    return typeof __APP_VERSION__ !== 'undefined' ? normalizeAppVersion(__APP_VERSION__) : '';
  } catch {
    return '';
  }
}
