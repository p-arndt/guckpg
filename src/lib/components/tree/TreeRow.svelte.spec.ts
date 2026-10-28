import { render, screen } from '@testing-library/svelte';
import { expect, it } from 'vitest';
import TreeRow from './TreeRow.svelte';

it('keeps a consistent row height when the tree overflows', () => {
	render(TreeRow, { depth: 2, label: 'customers' });
	const row = screen.getByRole('treeitem', { name: 'customers' });
	expect(row.classList.contains('shrink-0')).toBe(true);
	expect(row.classList.contains('h-[26px]')).toBe(true);
});
