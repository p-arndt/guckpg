<script lang="ts">
	import { errorMessage, invoke } from '#lib/api.js';
	import { confirmation } from '#lib/state/confirm.svelte.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Input } from '#lib/components/ui/input/index.js';

	let {
		open = $bindable(false),
		id,
		database
	}: { open: boolean; id: string; database: string } = $props();
	let path = $state('');
	let busy = $state(false);
	let message = $state('');

	async function execute(restore: boolean) {
		if (!path.trim() || busy) return;
		const target = { id, database, path };
		if (
			restore &&
			!(await confirmation.ask({
				title: `Restore ${target.database}?`,
				description:
					'This replaces matching database objects and their data with the archive contents. Restore only archives you trust.',
				confirmLabel: 'Restore database',
				destructive: true
			}))
		)
			return;
		busy = true;
		message = '';
		try {
			await invoke(restore ? 'restore_database' : 'backup_database', target);
			message = restore ? 'Restore completed.' : 'Backup completed.';
		} catch (error) {
			message = errorMessage(error);
		} finally {
			busy = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Backup and restore: {database}</Dialog.Title>
			<Dialog.Description
				>Use a custom-format PostgreSQL archive and an absolute file path. PostgreSQL client tools
				must be installed locally. SSH connections are not supported.</Dialog.Description
			>
		</Dialog.Header>
		<label for="backup-path">Archive path</label>
		<Input id="backup-path" bind:value={path} placeholder="/path/to/backup.dump" disabled={busy} />
		{#if message}<p role="status">{message}</p>{/if}
		<Dialog.Footer>
			<Button variant="outline" disabled={busy || !path.trim()} onclick={() => execute(false)}
				>Back up</Button
			>
			<Button variant="destructive" disabled={busy || !path.trim()} onclick={() => execute(true)}
				>Restore</Button
			>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
