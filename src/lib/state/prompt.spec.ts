import { describe, expect, it } from 'vitest';
import { Prompt } from './prompt.svelte';

describe('prompt', () => {
	it('resolves with the trimmed answer', async () => {
		const prompt = new Prompt();
		const answer = prompt.ask({ title: 'Name', label: 'Folder name', initial: 'x' });
		expect(prompt.pending?.initial).toBe('x');
		prompt.answer('  reports  ');
		await expect(answer).resolves.toBe('reports');
		expect(prompt.pending).toBeNull();
	});

	it('treats blank answers and cancelling as null', async () => {
		const prompt = new Prompt();
		const blank = prompt.ask({ title: 't', label: 'l' });
		prompt.answer('   ');
		await expect(blank).resolves.toBeNull();
		const cancelled = prompt.ask({ title: 't', label: 'l' });
		prompt.answer(null);
		await expect(cancelled).resolves.toBeNull();
	});

	it('cancels an older prompt when a new one opens', async () => {
		const prompt = new Prompt();
		const first = prompt.ask({ title: '1', label: 'l' });
		void prompt.ask({ title: '2', label: 'l' });
		await expect(first).resolves.toBeNull();
		expect(prompt.pending?.title).toBe('2');
	});
});
