import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import ClausePanelTestWrapper from './ClausePanelTestWrapper.svelte';

describe('aggregate conditions', () => {
	it('adds an editable HAVING condition', async () => {
		render(ClausePanelTestWrapper);
		await fireEvent.click(screen.getByRole('button', { name: 'Aggregate condition' }));
		expect(screen.getByRole('combobox', { name: 'HAVING aggregate' })).toBeTruthy();
		expect(screen.getByRole('combobox', { name: 'HAVING column' })).toBeTruthy();
		expect(screen.getByRole('textbox', { name: 'HAVING value' })).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'Remove HAVING condition' }));
		expect(screen.queryByRole('combobox', { name: 'HAVING column' })).toBeNull();
	});
	it('lets a user enable DISTINCT for an aggregate', async () => {
		render(ClausePanelTestWrapper);
		const checkbox = screen.getByRole('checkbox', { name: 'Distinct aggregate values' });
		expect(checkbox.getAttribute('aria-checked')).toBe('false');
		await fireEvent.click(checkbox);
		expect(checkbox.getAttribute('aria-checked')).toBe('true');
	});
});
