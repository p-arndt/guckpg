import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import ResultsPanel from './ResultsPanel.svelte';

const result = (rowCount: number) => ({
	columns: [],
	rows: [],
	rowCount,
	truncated: false,
	source: null
});

const output = { results: [result(1), result(2)], elapsedMs: 5 };

describe('ResultsPanel', () => {
	it('associates each result with its executed statement', async () => {
		render(ResultsPanel, {
			running: false,
			error: null,
			output,
			sql: 'update a set n = 1; update b set n = 2; -- trailing comment'
		});
		const tabs = screen.getAllByRole('tab');
		expect(tabs[0].textContent).toContain('update a set n = 1');
		expect(tabs[1].textContent).toContain('update b set n = 2');
		await fireEvent.click(tabs[0]);
		expect(tabs[0].getAttribute('aria-selected')).toBe('true');
		expect(screen.getByText('Statement 1')).toBeTruthy();
	});

	it('does not label results when the statements cannot be matched', () => {
		render(ResultsPanel, { running: false, error: null, output, sql: 'select 1' });
		expect(screen.getAllByRole('tab')[0].textContent).not.toContain('select 1');
		expect(screen.queryByText('Statement 1')).toBeNull();
	});
});
