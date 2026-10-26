import { describe, expect, it } from 'vitest';
import { PendingRequest } from './pending.svelte';

describe('PendingRequest', () => {
	it('exposes the open question and resolves with the answer', async () => {
		const request = new PendingRequest<{ text: string }, number>(-1);
		expect(request.pending).toBeNull();
		const answer = request.ask({ text: 'q' });
		expect(request.pending).toEqual({ text: 'q' });
		request.answer(7);
		await expect(answer).resolves.toBe(7);
		expect(request.pending).toBeNull();
	});

	it('declines the open question when a new one replaces it', async () => {
		const request = new PendingRequest<string, number>(-1);
		const first = request.ask('a');
		const second = request.ask('b');
		await expect(first).resolves.toBe(-1);
		expect(request.pending).toBe('b');
		request.answer(1);
		await expect(second).resolves.toBe(1);
	});

	it('shares the answer with questions that join the open one', async () => {
		const request = new PendingRequest<string, number>(-1);
		const first = request.ask('a');
		const second = request.ask('a again', (open) => open === 'a');
		expect(request.pending).toBe('a');
		request.answer(3);
		await expect(Promise.all([first, second])).resolves.toEqual([3, 3]);
	});

	it('ignores an answer without a question', () => {
		const request = new PendingRequest<string, number>(-1);
		request.answer(1);
		expect(request.pending).toBeNull();
	});
});
