import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

const { tabs, connections } = vi.hoisted(() => ({
	tabs: {
		items: [
			{ id: 'one', title: 'users', kind: 'table', connectionId: 'prod', database: 'app' },
			{ id: 'two', title: 'users', kind: 'table', connectionId: 'dev', database: 'test' }
		],
		activeId: 'one',
		closed: []
	},
	connections: { get: (id: string) => ({ name: id === 'prod' ? 'Production' : 'Development' }) }
}));
vi.mock('#lib/state/tabs.svelte.js', () => ({ tabs }));
vi.mock('#lib/state/connections.svelte.js', () => ({ connections }));

import TabBar from './TabBar.svelte';

describe('tab navigation', () => {
	it('shows the active context and offers every tab in the menu', async () => {
		Element.prototype.scrollIntoView = vi.fn();
		render(TabBar, { onNewQuery: vi.fn() });
		expect(screen.getByText('Production / app')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'All open tabs' }));
		expect(await screen.findByText('Development / test')).toBeTruthy();
		await fireEvent.click(screen.getByText('Development / test'));
		expect(tabs.activeId).toBe('two');
	});
});
