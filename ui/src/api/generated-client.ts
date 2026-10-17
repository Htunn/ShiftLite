import createClient from 'openapi-fetch';
import type { paths } from './schema';

// Generated from ../../api/openapi.yaml via `npm run generate-client` (ADR-005).
// Do not edit `schema.ts` by hand — regenerate it instead.
export const api = createClient<paths>({
  baseUrl: '/',
  credentials: 'include',
});

/** Thrown by `unwrap` when a request fails; carries the machine-readable error code. */
export class ApiRequestError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

/**
 * Unwraps an `openapi-fetch` response, throwing `ApiRequestError` on failure so callers can
 * branch on `error.code` to render the UI states required by SPEC-001 (one state per error code).
 */
export function unwrap<T>(result: { data?: T; error?: { code?: string; message?: string } }): T {
  if (result.error) {
    throw new ApiRequestError(result.error.code ?? 'internal', result.error.message ?? 'request failed');
  }
  return result.data as T;
}
