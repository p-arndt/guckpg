<script lang="ts">
	import { isTauri } from '@tauri-apps/api/core';
	import { check, type Update } from '@tauri-apps/plugin-updater';
	import { relaunch } from '@tauri-apps/plugin-process';
	import DownloadIcon from '@lucide/svelte/icons/download';
	import { errorMessage } from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';

	let open = $state(false);
	let busy = $state(false);
	let update = $state<Update | null>(null);
	let message = $state('');
	let installed = $state(false);
	let error = $state('');

	async function checkForUpdates() {
		open = true;
		busy = true;
		update = null;
		installed = false;
		message = '';
		error = '';
		try {
			update = await check();
			if (!update) message = 'You are up to date.';
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
		}
	}

	async function install() {
		if (!update || busy) return;
		busy = true;
		error = '';
		try {
			if (!installed) {
				await update.downloadAndInstall();
				installed = true;
			}
			await relaunch();
		} catch (e) {
			error = errorMessage(e);
			busy = false;
		}
	}
</script>

{#if isTauri()}
	<Button
		size="icon-sm"
		variant="ghost"
		aria-label="Check for updates"
		title="Check for updates"
		onclick={checkForUpdates}
	>
		<DownloadIcon />
	</Button>
	<Dialog.Root bind:open>
		<Dialog.Content
			onInteractOutside={(event) => busy && event.preventDefault()}
			onEscapeKeydown={(event) => busy && event.preventDefault()}
			showCloseButton={!busy}
		>
			<Dialog.Header>
				<Dialog.Title>Software update</Dialog.Title>
				<Dialog.Description>
					{#if busy}
						{installed ? 'Restarting…' : update ? 'Installing update…' : 'Checking for updates…'}
					{:else if installed}
						The update is installed. Restart guckpg to finish updating.
					{:else if update}
						Version {update.version} is available. Installing it will restart guckpg. Save any unsaved
						changes and finish running queries first.
					{:else}
						{message || 'Could not check for updates.'}
					{/if}
				</Dialog.Description>
			</Dialog.Header>
			{#if error}<p role="alert" class="text-sm text-bad">{error}</p>{/if}
			{#if update && !busy}
				<Dialog.Footer>
					<Button variant="outline" onclick={() => (open = false)}>Later</Button>
					<Button onclick={install}>{installed ? 'Restart' : 'Install and restart'}</Button>
				</Dialog.Footer>
			{/if}
		</Dialog.Content>
	</Dialog.Root>
{/if}
