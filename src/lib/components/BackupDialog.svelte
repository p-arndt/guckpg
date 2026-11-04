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
	let sql = $state(false);
	let tables = $state('');
	let contents = $state('all');
	let clean = $state(false);
	let replace = $state(true);
	let mode = $state<'export' | 'restore'>('export');

	async function choosePath() {
		try {
			const selected = await invoke<string | null>('choose_file', {
				save: mode === 'export',
				filename: sql ? 'backup.sql' : 'backup.dump',
				extension: sql ? 'sql' : 'dump'
			});
			if (selected) path = selected;
		} catch (error) {
			message = errorMessage(error);
		}
	}

	async function execute(restore: boolean) {
		if (!path.trim() || busy) return;
		const target = { id, database, path, sql };
		const tablePatterns = tables
			.split('\n')
			.map((table) => table.trim())
			.filter(Boolean);
		if (
			restore &&
			!(await confirmation.ask({
				title: `Restore ${target.database}?`,
				description: sql
					? 'This runs the SQL file against the selected database and can also execute commands on this computer. It may replace or delete data. Run only files you trust.'
					: replace
						? 'This replaces matching database objects and their data with the archive contents. Restore only archives you trust.'
						: 'This restores the archive into the selected database and stops if objects already exist. Restore only archives you trust.',
				confirmLabel: 'Restore database',
				destructive: true
			}))
		)
			return;
		busy = true;
		message = '';
		try {
			await invoke(
				restore ? 'restore_database' : 'backup_database',
				restore
					? { ...target, replace }
					: { id, database, path, options: { sql, tables: tablePatterns, contents, clean } }
			);
			message = restore ? 'Restore completed.' : 'Backup completed.';
		} catch (error) {
			message = errorMessage(error);
		} finally {
			busy = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="max-h-[90vh] overflow-y-auto">
		<Dialog.Header>
			<Dialog.Title>Backup and restore: {database}</Dialog.Title>
			<Dialog.Description
				>Use an absolute file path. PostgreSQL client tools must be installed locally. SSH
				connections are not supported.</Dialog.Description
			>
		</Dialog.Header>
		<fieldset class="flex gap-5" disabled={busy}>
			<legend class="mb-2 font-medium">Action</legend>
			<label><input type="radio" bind:group={mode} value="export" /> Export</label>
			<label><input type="radio" bind:group={mode} value="restore" /> Restore</label>
		</fieldset>
		<label for="backup-format">Format</label>
		<select
			id="backup-format"
			bind:value={sql}
			disabled={busy}
			class="rounded-md border bg-background p-2"
		>
			<option value={false}>Custom archive</option>
			<option value={true}>SQL file</option>
		</select>
		<label for="backup-path">File path</label>
		<div class="flex gap-2">
			<Input
				id="backup-path"
				bind:value={path}
				placeholder={sql ? '/path/to/backup.sql' : '/path/to/backup.dump'}
				disabled={busy}
			/>
			<Button variant="outline" disabled={busy} onclick={choosePath}>Browse…</Button>
		</div>
		{#if mode === 'export'}
			<label for="backup-contents">Export contents</label>
			<select
				id="backup-contents"
				bind:value={contents}
				disabled={busy}
				class="rounded-md border bg-background p-2"
			>
				<option value="all">Schema and data</option>
				<option value="schema">Schema only</option>
				<option value="data">Data only</option>
			</select>
			<label for="backup-tables"
				>Tables to export (optional; one PostgreSQL table pattern per line)</label
			>
			<textarea
				id="backup-tables"
				bind:value={tables}
				disabled={busy}
				class="rounded-md border bg-background p-2"
				placeholder="public.my_table"></textarea>
			{#if sql}
				<label
					><input type="checkbox" bind:checked={clean} disabled={busy || contents === 'data'} /> Include
					DROP statements in SQL export</label
				>
			{/if}
		{:else if !sql}
			<label
				><input type="checkbox" bind:checked={replace} disabled={busy} /> Replace matching objects when
				restoring</label
			>
		{/if}
		{#if busy}<p role="status">Working…</p>{:else if message}<p role="status">{message}</p>{/if}
		<Dialog.Footer>
			{#if mode === 'export'}
				<Button disabled={busy || !path.trim()} onclick={() => execute(false)}>Export</Button>
			{:else}
				<Button variant="destructive" disabled={busy || !path.trim()} onclick={() => execute(true)}
					>Restore</Button
				>
			{/if}
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
