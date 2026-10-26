<script lang="ts">
	import { errorMessage, invoke } from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Input } from '#lib/components/ui/input/index.js';

	let {
		open = $bindable(false),
		connectionId,
		database,
		schema,
		table,
		readOnly,
		onInserted
	}: {
		open: boolean;
		connectionId: string;
		database: string;
		schema: string;
		table: string;
		readOnly: boolean;
		onInserted: () => void;
	} = $props();
	let count = $state(10);
	let sql = $state('');
	let previewCount = $state(0);
	let busy = $state(false);
	let message = $state('');

	async function preview() {
		busy = true;
		message = '';
		sql = '';
		try {
			const requested = Number(count);
			sql = await invoke<string>('preview_test_data', {
				id: connectionId,
				database,
				schema,
				table,
				count: requested
			});
			previewCount = requested;
		} catch (error) {
			message = errorMessage(error);
		} finally {
			busy = false;
		}
	}

	async function insert() {
		if (readOnly || !sql || count !== previewCount) return;
		busy = true;
		message = '';
		try {
			const inserted = await invoke<number>('insert_test_data', {
				id: connectionId,
				database,
				schema,
				table,
				count: previewCount,
				expectedSql: sql
			});
			sql = '';
			message = `Inserted ${inserted} rows.`;
			onInserted();
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
			<Dialog.Title>Generate test data: {schema}.{table}</Dialog.Title>
			<Dialog.Description
				>Generate values for required columns without defaults. Existing rows remain in place;
				constraints and triggers still apply.</Dialog.Description
			>
		</Dialog.Header>
		<label for="test-data-count">Rows (1–1000)</label>
		<Input
			id="test-data-count"
			type="number"
			min="1"
			max="1000"
			bind:value={count}
			disabled={busy}
		/>
		{#if sql && count === previewCount}
			<p class="text-sm">Review the SQL before inserting:</p>
			<pre class="max-h-48 overflow-auto rounded border p-2 text-xs whitespace-pre-wrap">{sql}</pre>
		{/if}
		{#if message}<p role="status">{message}</p>{/if}
		<Dialog.Footer>
			<Button
				variant="outline"
				disabled={busy || !Number.isInteger(count) || count < 1 || count > 1000}
				onclick={preview}>Preview</Button
			>
			<Button disabled={busy || readOnly || !sql || count !== previewCount} onclick={insert}
				>Insert {previewCount || ''} rows</Button
			>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
