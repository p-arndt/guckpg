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
	let input: HTMLInputElement;
	let busy = $state(false);
	let error = $state('');

	async function importFile(event: Event) {
		const element = event.currentTarget as HTMLInputElement;
		const file = element.files?.[0];
		element.value = '';
		if (!file) return;
		error = '';
		if (file.size > 16 * 1024 * 1024) {
			error = 'CSV file exceeds the 16 MiB limit.';
			return;
		}
		busy = true;
		try {
			const csv = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
			await invoke<number>('import_csv', { id: connectionId, database, schema, table, csv });
			onImported();
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
		}
	}

	async function exportFile() {
		error = '';
		busy = true;
		try {
			const csv = await invoke<string>('export_csv', { id: connectionId, database, schema, table });
			const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
			const link = document.createElement('a');
			link.href = url;
			link.download = `${table}.csv`;
			link.click();
			setTimeout(() => URL.revokeObjectURL(url), 1000);
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
		}
	}
</script>

<input bind:this={input} type="file" accept=".csv,text/csv" class="hidden" onchange={importFile} />
<Button
	size="sm"
	variant="outline"
	disabled={disabled || busy || readOnly}
	onclick={() => input.click()}
	title="Import CSV with a header row">Import CSV</Button
>
<Button
	size="sm"
	variant="outline"
	disabled={disabled || busy}
	onclick={exportFile}
	title="Export the entire table as CSV">Export CSV</Button
>
{#if error}<span role="alert" class="text-xs text-bad">{error}</span>{/if}
