import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const { errorMessage, isAppError } = await import('./api');

describe('isAppError', () => {
	it('recognizes backend errors by kind', () => {
		expect(isAppError({ kind: 'message', message: 'x' })).toBe(true);
		expect(isAppError({ kind: 'unknownHostKey', host: 'h' })).toBe(true);
		expect(isAppError({ kind: 'other' })).toBe(false);
		expect(isAppError('boom')).toBe(false);
		expect(isAppError(null)).toBe(false);
	});
});

describe('errorMessage', () => {
	it('uses the message of backend errors', () => {
		expect(
			errorMessage({
				kind: 'postgres',
				message: 'relation "x" does not exist',
				code: '42P01',
				detail: null,
				hint: null,
				position: 15
			})
		).toBe('relation "x" does not exist');
	});

	it('warns loudly about changed host keys', () => {
		expect(
			errorMessage({ kind: 'hostKeyChanged', host: 'vps', port: 22, fingerprint: 'SHA256:x' })
		).toContain('CHANGED');
	});

	it('handles plain errors and strings', () => {
		expect(errorMessage(new Error('a'))).toBe('a');
		expect(errorMessage('b')).toBe('b');
	});
});
