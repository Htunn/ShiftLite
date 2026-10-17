import { describe, expect, it } from 'vitest';
import { ApiRequestError, unwrap } from './generated-client';

describe('unwrap', () => {
  it('returns data when there is no error', () => {
    const result = unwrap({ data: { name: 'auth-service' } });
    expect(result).toEqual({ name: 'auth-service' });
  });

  it('throws ApiRequestError carrying the machine-readable code on failure', () => {
    expect(() => unwrap({ error: { code: 'builder_not_supported', message: 'nope' } })).toThrow(ApiRequestError);
    try {
      unwrap({ error: { code: 'builder_not_supported', message: 'nope' } });
    } catch (e) {
      expect(e).toBeInstanceOf(ApiRequestError);
      expect((e as ApiRequestError).code).toBe('builder_not_supported');
      expect((e as ApiRequestError).message).toBe('nope');
    }
  });

  it('falls back to an internal error code when the server omits one', () => {
    try {
      unwrap({ error: {} });
    } catch (e) {
      expect((e as ApiRequestError).code).toBe('internal');
    }
  });
});
