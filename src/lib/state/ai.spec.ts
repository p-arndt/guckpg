import { beforeEach, describe, expect, it, vi } from 'vitest';
import { aiApi, type AiReply } from '#lib/ai.js';
import { AiChat } from './ai.svelte.js';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason: unknown) => void;
	const promise = new Promise<T>((yes, no) => {
		resolve = yes;
		reject = no;
	});
	return { promise, resolve, reject };
}

function chat() {
	const state = new AiChat();
	state.loaded = true;
	state.setContext({ connectionId: 'one', database: 'db' });
	return state;
}

beforeEach(() => {
	vi.restoreAllMocks();
	vi.spyOn(aiApi, 'cancel').mockResolvedValue();
});

describe('chat isolation', () => {
	it('pins each request and ignores a reply after database changes', async () => {
		const pending = deferred<AiReply>();
		vi.spyOn(aiApi, 'chat').mockReturnValue(pending.promise);
		const state = chat();
		state.setAllowData(true);
		const first = state.send('Explain tables');
		const request = vi.mocked(aiApi.chat).mock.calls[0][0];
		expect(request).toMatchObject({ connectionId: 'one', database: 'db', allowData: true });
		state.setContext({ connectionId: 'one', database: 'other' });
		expect(aiApi.cancel).toHaveBeenCalledWith(request.requestId);
		pending.resolve({ text: 'Old answer', tools: [] });
		await first;
		expect(state.messages).toEqual([]);
		expect(state.busy).toBe(false);
		expect(state.allowData).toBe(false);
	});
	it('cancels without accepting late text and lets a newer request finish', async () => {
		const old = deferred<AiReply>();
		const current = deferred<AiReply>();
		vi.spyOn(aiApi, 'chat').mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
		const state = chat();
		const first = state.send('First');
		await state.cancel();
		const second = state.send('Second');
		expect(vi.mocked(aiApi.chat).mock.calls[1][0].messages).toEqual([
			{ role: 'user', content: 'Second' }
		]);
		old.resolve({ text: 'Stale', tools: [] });
		await first;
		expect(state.busy).toBe(true);
		current.resolve({ text: 'Current', tools: [{ name: 'list_tables', summary: '2 tables' }] });
		await second;
		expect(state.messages.at(-1)).toMatchObject({
			content: 'Current',
			tools: [{ name: 'list_tables', summary: '2 tables' }]
		});
		expect(state.messages.some((m) => m.content === 'Stale')).toBe(false);
	});
	it('clears sampled history when data permission is revoked', async () => {
		vi.spyOn(aiApi, 'chat').mockResolvedValue({ text: 'Sampled row', tools: [] });
		const state = chat();
		state.setAllowData(true);
		await state.send('Example');
		expect(vi.mocked(aiApi.chat).mock.calls[0][0].allowData).toBe(true);
		state.setAllowData(false);
		expect(state.messages).toEqual([]);
		expect(state.allowData).toBe(false);
	});
	it('preserves history for the same context and resets on provider settings changes', async () => {
		vi.spyOn(aiApi, 'chat').mockResolvedValue({ text: 'Answer', tools: [] });
		const state = chat();
		await state.send('Question');
		state.setContext({ connectionId: 'one', database: 'db' });
		expect(state.messages).toHaveLength(2);
		state.reset();
		expect(state.messages).toEqual([]);
	});
	it('shows request errors without retrying or accepting stale cancellation errors', async () => {
		vi.spyOn(aiApi, 'chat').mockRejectedValue({ kind: 'message', message: 'Provider unavailable' });
		const state = chat();
		await state.send('Question');
		expect(state.error).toBe('Provider unavailable');
		expect(aiApi.chat).toHaveBeenCalledTimes(1);
		const pending = deferred<AiReply>();
		const cancel = deferred<void>();
		vi.mocked(aiApi.chat).mockReturnValue(pending.promise);
		vi.mocked(aiApi.cancel).mockReturnValue(cancel.promise);
		const sent = state.send('Another');
		expect(vi.mocked(aiApi.chat).mock.calls[1][0].messages).toEqual([
			{ role: 'user', content: 'Another' }
		]);
		state.setContext(null);
		cancel.reject(new Error('Late cancel error'));
		pending.resolve({ text: 'Old', tools: [] });
		await sent;
		expect(state.error).toBe('');
	});
});
