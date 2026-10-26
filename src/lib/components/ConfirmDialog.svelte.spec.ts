import { fireEvent, render, screen } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { describe, expect, it } from 'vitest';
import ConfirmDialog from './ConfirmDialog.svelte';
import { confirmation } from '#lib/state/confirm.svelte.js';

describe('ConfirmDialog', () => {
	it('shows the pending request and resolves with the chosen answer', async () => {
		render(ConfirmDialog);
		expect(screen.queryByRole('alertdialog')).toBeNull();

		const answer = confirmation.ask({
			title: 'Drop table?',
			description: 'This cannot be undone.',
			confirmLabel: 'Drop'
		});
		flushSync();

		expect(await screen.findByRole('alertdialog')).toBeTruthy();
		expect(screen.getByText('Drop table?')).toBeTruthy();
		expect(screen.getByText('This cannot be undone.')).toBeTruthy();

		await fireEvent.click(screen.getByRole('button', { name: 'Drop' }));
		expect(await answer).toBe(true);
		expect(confirmation.pending).toBeNull();
	});

	it('answers false on cancel', async () => {
		render(ConfirmDialog);
		const answer = confirmation.ask({
			title: 'Discard?',
			description: 'Unsaved edits are lost.',
			confirmLabel: 'Discard'
		});
		flushSync();

		await fireEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
		expect(await answer).toBe(false);
	});
});
