import { describe, expect, it } from 'vitest';
import { confirmDiscard, confirmation } from './confirm.svelte.js';

describe('confirmation', () => {
	it('resolves with the answer', async () => {
		const answer = confirmation.ask({ title: 't', description: 'd', confirmLabel: 'ok' });
		expect(confirmation.pending?.title).toBe('t');
		confirmation.answer(true);
		await expect(answer).resolves.toBe(true);
		expect(confirmation.pending).toBeNull();
	});

	it('declines an older question when a new one comes', async () => {
		const first = confirmation.ask({ title: '1', description: '', confirmLabel: 'ok' });
		const second = confirmation.ask({ title: '2', description: '', confirmLabel: 'ok' });
		await expect(first).resolves.toBe(false);
		confirmation.answer(true);
		await expect(second).resolves.toBe(true);
	});

	it('does not ask when there is nothing to discard', async () => {
		await expect(confirmDiscard(0)).resolves.toBe(true);
		expect(confirmation.pending).toBeNull();
		const asked = confirmDiscard(3);
		expect(confirmation.pending?.title).toBe('Discard 3 unsaved changes?');
		confirmation.answer(false);
		await expect(asked).resolves.toBe(false);
	});
});
