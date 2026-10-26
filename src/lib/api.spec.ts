import { beforeEach, describe, expect, it, vi } from 'vitest';

const { tauriInvoke } = vi.hoisted(() => ({ tauriInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: tauriInvoke }));

const { errorMessage, invoke, isAppError, toAppError } = await import('./api');
const { hostKeyPrompt } = await import('./state/host-key.svelte');

describe('isAppError', () => {
	it('recognizes backend errors by kind', () => {
		expect(isAppError({ kind: 'message', message: 'x' })).toBe(true);
		expect(isAppError({ kind: 'unknownHostKey', host: 'h' })).toBe(true);
		expect(isAppError({ kind: 'other' })).toBe(false);
		expect(isAppError('boom')).toBe(false);
		expect(isAppError(null)).toBe(false);
	});
});

describe('toAppError', () => {
	it('keeps backend errors and wraps everything else', () => {
		const backend = { kind: 'message', message: 'x' } as const;
		expect(toAppError(backend)).toBe(backend);
		expect(toAppError(new Error('a'))).toEqual({ kind: 'message', message: 'a' });
		expect(toAppError('b')).toEqual({ kind: 'message', message: 'b' });
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

describe('invoke', () => {
	const unknownKey = {
		kind: 'unknownHostKey',
		host: 'vps',
		port: 22,
		fingerprint: 'SHA256:x',
		key: 'ssh-ed25519 AAAA'
	};

	beforeEach(() => {
		tauriInvoke.mockReset();
	});

	it('passes results and other errors through', async () => {
		tauriInvoke.mockResolvedValueOnce(['db']);
		await expect(invoke('list_databases', { id: 'c' })).resolves.toEqual(['db']);
		tauriInvoke.mockRejectedValueOnce({ kind: 'message', message: 'down' });
		await expect(invoke('connect', { id: 'c' })).rejects.toEqual({
			kind: 'message',
			message: 'down'
		});
		expect(hostKeyPrompt.pending).toBeNull();
	});

	it('trusts an accepted host key and retries once', async () => {
		tauriInvoke.mockRejectedValueOnce(unknownKey);
		tauriInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(cmd === 'connect' ? { version: '17', database: 'db' } : undefined)
		);
		const result = invoke('connect', { id: 'c' });
		await vi.waitFor(() => expect(hostKeyPrompt.pending?.host).toBe('vps'));
		hostKeyPrompt.answer(true);
		await expect(result).resolves.toEqual({ version: '17', database: 'db' });
		expect(tauriInvoke.mock.calls).toEqual([
			['connect', { id: 'c' }],
			['trust_host_key', { host: 'vps', port: 22, key: 'ssh-ed25519 AAAA' }],
			['connect', { id: 'c' }]
		]);
	});

	it('throws the host key error when the user declines', async () => {
		tauriInvoke.mockRejectedValueOnce(unknownKey);
		const result = invoke('connect', { id: 'c' });
		await vi.waitFor(() => expect(hostKeyPrompt.pending).not.toBeNull());
		hostKeyPrompt.answer(false);
		await expect(result).rejects.toBe(unknownKey);
		expect(tauriInvoke).toHaveBeenCalledTimes(1);
	});
});
