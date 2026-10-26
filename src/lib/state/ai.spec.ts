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
	it('shows thinking and tools in order and approves each write only once', async () => {
		const pending = deferred<AiReply>();
		vi.spyOn(aiApi, 'chat').mockReturnValue(pending.promise);
		vi.spyOn(aiApi, 'decideWrite').mockResolvedValue();
		const state = chat();
		state.setAccess('write');
		const sent = state.send('Change a row');
		const [request, event] = vi.mocked(aiApi.chat).mock.calls[0];
		expect(request).toMatchObject({ allowData: true, allowWrite: true });
		event({ type: 'thinking', text: 'First' });
		event({ type: 'toolStarted', name: 'list_tables' });
		event({ type: 'toolFinished', summary: 'Completed', detail: '{"tables": []}' });
		expect(state.activity.at(-1)).toMatchObject({ summary: 'Completed', detail: '{"tables": []}' });
		event({ type: 'thinking', text: 'Next' });
		event({ type: 'toolStarted', name: 'propose_write' });
		event({ type: 'approval', id: 'approval', sql: 'UPDATE test SET value = 1' });
		expect(state.activity.map((item) => item.type)).toEqual([
			'thinking',
			'tool',
			'thinking',
			'tool'
		]);
		await state.decideWrite('approval', true);
		await state.decideWrite('approval', true);
		expect(aiApi.decideWrite).toHaveBeenCalledTimes(1);
		expect(aiApi.decideWrite).toHaveBeenCalledWith(request.requestId, 'approval', true);
		event({ type: 'toolFinished', summary: 'Completed · 1 rows affected' });
		pending.resolve({ text: 'Done', tools: [] });
		await sent;
		expect(state.messages.at(-1)?.activity).toHaveLength(4);
		event({ type: 'thinking', text: 'late' });
		expect(state.activity).toEqual([]);
	});
	it('maps access levels onto read and write permissions', () => {
		const state = chat();
		expect(state.access).toBe('schema');
		state.setAccess('write');
		expect(state.allowData).toBe(true);
		expect(state.allowWrite).toBe(true);
		expect(state.access).toBe('write');
		state.setAccess('read');
		expect(state.allowData).toBe(true);
		expect(state.allowWrite).toBe(false);
		state.setAccess('schema');
		expect(state.allowData).toBe(false);
		expect(state.allowWrite).toBe(false);
	});
	it('enables confirmation for every database access', async () => {
		const state = chat();
		vi.spyOn(aiApi, 'chat').mockResolvedValue({ text: 'Done', tools: [] });
		state.setAccess('confirm');
		await state.send('Inspect the database');
		expect(vi.mocked(aiApi.chat).mock.calls[0][0]).toMatchObject({
			allowData: true,
			allowWrite: true,
			confirmReads: true
		});
		state.setAccess('read');
		expect(state.confirmReads).toBe(false);
		state.setAccess('confirm', true);
		expect(state.allowWrite).toBe(false);
		expect(state.access).toBe('confirm');
	});

	it('clears the chat only when the access level changes', () => {
		const state = chat();
		state.setAccess('read');
		state.messages = [{ role: 'user', content: 'hi' }];
		state.setAccess('read');
		expect(state.messages).toHaveLength(1);
		state.setAccess('write');
		expect(state.messages).toEqual([]);
	});
	it('pins each request and ignores a reply after database changes', async () => {
		const pending = deferred<AiReply>();
		vi.spyOn(aiApi, 'chat').mockReturnValue(pending.promise);
		const state = chat();
		state.setAccess('read');
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
		state.setAccess('read');
		await state.send('Example');
		expect(vi.mocked(aiApi.chat).mock.calls[0][0].allowData).toBe(true);
		state.setAccess('schema');
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
