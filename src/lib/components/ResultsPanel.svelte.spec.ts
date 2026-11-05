import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.stubGlobal(
	'ResizeObserver',
	class {
		observe() {}
		unobserve() {}
		disconnect() {}
	}
);
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
	it('pages through cached rows without running SQL again', async () => {
		invoke.mockResolvedValueOnce([['third']]);
		render(ResultsPanel, {
			running: false,
			error: null,
			tabId: 'tab-1',
			output: {
				cacheId: 'cache-1',
				elapsedMs: 1,
				results: [
					{
						columns: ['value'],
						rows: [['first'], ['second']],
						rowCount: 3,
						truncated: true,
						source: null
					}
				]
			}
		});
		expect(screen.getByText('Page 1 of 2 · 3 rows total')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'Next page' }));
		expect(await screen.findByText('third')).toBeTruthy();
		expect(invoke).toHaveBeenCalledWith('query_result_page', {
			tabId: 'tab-1',
			cacheId: 'cache-1',
			result: 0,
			page: 1
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Previous page' }));
		expect(screen.getByText('first')).toBeTruthy();
	});

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
