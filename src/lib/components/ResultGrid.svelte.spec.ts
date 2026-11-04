import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import ResultGrid from './ResultGrid.svelte';

vi.stubGlobal(
	'ResizeObserver',
	class {
		observe() {}
		unobserve() {}
		disconnect() {}
	}
);

describe('ResultGrid', () => {
	it('selects and copies all displayed rows with the keyboard', async () => {
		const writeText = vi.fn().mockResolvedValue(undefined);
		Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
		render(ResultGrid, { columns: ['name'], rows: [['first'], ['second']] });
		const grid = screen.getByRole('grid');
		await fireEvent.keyDown(grid, { key: 'a', ctrlKey: true });
		await fireEvent.keyDown(grid, { key: 'c', ctrlKey: true });
		expect(writeText).toHaveBeenCalledWith('first\nsecond');
	});

	it('generates a UUID for a new UUID cell through its context menu', async () => {
		render(ResultGrid, {
			columns: ['id'],
			rows: [],
			columnTypes: { id: 'uuid' },
			source: { schema: 'public', table: 'items', columns: ['id'], key: [0] },
			editTarget: { connectionId: 'test', database: 'test', readOnly: false }
		});
		await fireEvent.click(screen.getByTitle('Add a row'));
		await fireEvent.contextMenu(screen.getByRole('gridcell'));
		await fireEvent.click(await screen.findByText('Generate UUID'));
		expect(screen.getByRole('gridcell').textContent).toMatch(
			/[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/i
		);
	});
});
