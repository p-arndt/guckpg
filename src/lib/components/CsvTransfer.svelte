<script lang="ts">
	import { invoke, errorMessage } from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';

	interface Props {
		connectionId: string;
		database: string;
		schema: string;
		table: string;
		readOnly: boolean;
		disabled?: boolean;
		onImported: () => void;
	}
	let {
		connectionId,
		database,
		schema,
		table,
		readOnly,
		disabled = false,
		onImported
	}: Props = $props();
	let busy = $state(false);
	let error = $state('');

	async function transfer(save: boolean) {
		error = '';
		busy = true;
		try {
			const path = await invoke<string | null>('choose_file', {
				save,
				filename: `${table}.csv`,
				extension: 'csv'
			});
			if (!path) return;
			await invoke(save ? 'export_csv' : 'import_csv', {
				id: connectionId,
				database,
				schema,
				table,
				path
			});
			if (!save) onImported();
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
		}
	}
</script>

<Button
	size="sm"
	variant="outline"
	disabled={disabled || busy || readOnly}
	onclick={() => transfer(false)}
	title="Import CSV with a header row">Import CSV</Button
>
<Button
	size="sm"
	variant="outline"
	disabled={disabled || busy}
	onclick={() => transfer(true)}
	title="Export the entire table as CSV">Export CSV</Button
>
{#if error}<span role="alert" class="text-xs text-bad">{error}</span>{/if}
