import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import UpdateButton from './UpdateButton.svelte';

const { check, relaunch, downloadAndInstall } = vi.hoisted(() => ({
	check: vi.fn(),
	relaunch: vi.fn(),
	downloadAndInstall: vi.fn()
}));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true, invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-updater', () => ({ check }));
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch }));

beforeEach(() => {
	vi.resetAllMocks();
});

describe('updates', () => {
	it('reports when the app is up to date', async () => {
		check.mockResolvedValue(null);
		render(UpdateButton);
		await fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
		expect(await screen.findByText('You are up to date.')).toBeTruthy();
	});

	it('installs only after confirmation and restarts', async () => {
		check.mockResolvedValue({ version: '1.2.3', downloadAndInstall });
		downloadAndInstall.mockResolvedValue(undefined);
		relaunch.mockResolvedValue(undefined);
		render(UpdateButton);
		await fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
		expect(await screen.findByText(/Version 1.2.3 is available/)).toBeTruthy();
		expect(downloadAndInstall).not.toHaveBeenCalled();
		await fireEvent.click(screen.getByRole('button', { name: 'Install and restart' }));
		await waitFor(() => expect(relaunch).toHaveBeenCalledOnce());
	});

	it('shows an installation error without restarting', async () => {
		check.mockResolvedValue({ version: '1.2.3', downloadAndInstall });
		downloadAndInstall.mockRejectedValue(new Error('Download failed'));
		render(UpdateButton);
		await fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Install and restart' }));
		expect((await screen.findByRole('alert')).textContent).toContain('Download failed');
		expect(relaunch).not.toHaveBeenCalled();
	});

	it('shows check errors and allows retrying a failed restart without reinstalling', async () => {
		check.mockRejectedValueOnce(new Error('Network unavailable'));
		render(UpdateButton);
		await fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
		expect((await screen.findByRole('alert')).textContent).toContain('Network unavailable');
		await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
		check.mockResolvedValue({ version: '1.2.3', downloadAndInstall });
		downloadAndInstall.mockResolvedValue(undefined);
		relaunch.mockRejectedValueOnce(new Error('Restart failed')).mockResolvedValueOnce(undefined);
		await fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Install and restart' }));
		expect((await screen.findByRole('alert')).textContent).toContain('Restart failed');
		await fireEvent.click(screen.getByRole('button', { name: 'Restart' }));
		await waitFor(() => expect(relaunch).toHaveBeenCalledTimes(2));
		expect(downloadAndInstall).toHaveBeenCalledOnce();
	});
});
