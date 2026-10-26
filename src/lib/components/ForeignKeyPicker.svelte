<script lang="ts">
	import { api, errorMessage, type ForeignKey } from '#lib/api.js';
	import { referenceQuery } from '#lib/foreign-key-picker.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { toast } from 'svelte-sonner';

	let {
		connectionId,
		database,
		schema,
		reference,
		onSelect
	}: {
		connectionId: string;
		database: string;
		schema: string;
		reference: ForeignKey;
		onSelect: (value: string) => void;
	} = $props();
	let values = $state<string[] | null>(null);
	let more = $state(false);
	let loading = $state(false);

	async function load() {
		loading = true;
		const tabId = `fk-picker-${crypto.randomUUID()}`;
		try {
			const output = await api.runQuery(
				tabId,
				connectionId,
				database,
				referenceQuery(schema, reference),
				101
			);
			const rows = output.results[0]?.rows ?? [];
			more = rows.length > 100;
			values = rows
				.slice(0, 100)
				.map((row) => row[0])
				.filter((value): value is string => value !== null);
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			loading = false;
			void api.closeTab(tabId).catch(() => {});
		}
	}
</script>

<div class="mt-1 flex items-center gap-2">
	<Button size="sm" variant="outline" disabled={loading} onclick={load}
		>{loading ? 'Loading…' : `Choose from ${reference.toTable}`}</Button
	>
	{#if values !== null}
		<select
			class="min-w-0 flex-1 rounded-lg border border-input bg-background px-2 py-1 text-xs"
			aria-label={`Choose ${reference.toTable}.${reference.toColumns[0]}`}
			value=""
			onchange={(event) => {
				const index = Number(event.currentTarget.value);
				if (event.currentTarget.value !== '' && values?.[index] !== undefined)
					onSelect(values[index]);
				event.currentTarget.value = '';
			}}
		>
			<option value="">Select a value</option>
			{#each values as value, index (index)}<option value={String(index)}>{value}</option>{/each}
		</select>
	{/if}
</div>
{#if values !== null && more}<p class="mt-1 text-xs text-muted-foreground">
		Showing the first 100 values. You can enter another value manually.
	</p>{/if}
