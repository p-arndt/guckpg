import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { version } from '../../../package.json';
import TopBarTestWrapper from './TopBarTestWrapper.svelte';

describe('sidebar toggle', () => {
	it('exposes the sidebar state and calls the toggle action', async () => {
		const onToggleSidebar = vi.fn();
		const view = render(TopBarTestWrapper, {
			sidebarOpen: true,
			onToggleSidebar
		});
		expect(screen.getByText('pieg')).toBeTruthy();
		expect(screen.getByText(`v${version}`)).toBeTruthy();
		const toggle = screen.getByRole('button', { name: 'Hide sidebar' });
		expect(toggle.getAttribute('aria-expanded')).toBe('true');
		await fireEvent.click(toggle);
		expect(onToggleSidebar).toHaveBeenCalledOnce();
		await view.rerender({ sidebarOpen: false });
		expect(screen.getByRole('button', { name: 'Show sidebar' }).getAttribute('aria-expanded')).toBe(
			'false'
		);
	});
});
