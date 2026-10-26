<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { api, errorMessage } from '#lib/api.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { catalog } from '#lib/state/catalog.js';
	import {
		compareSchemas,
		type SchemaDifference,
		type SchemaSnapshot
	} from '#lib/schema-compare.js';

	let {
		open = $bindable(),
		sourceId,
		sourceDatabase
	}: { open: boolean; sourceId: string; sourceDatabase: string } = $props();
	let targetId = $state('');
	let targetDatabase = $state('');
	let sourceSchema = $state('public');
	let targetSchema = $state('public');
	let busy = $state(false);
	let error = $state('');
	let differences = $state<SchemaDifference[] | null>(null);
	$effect(() => {
		if (open) {
			targetId = sourceId;
			targetDatabase = sourceDatabase;
			sourceSchema = 'public';
			targetSchema = 'public';
			differences = null;
			error = '';
		}
	});
	async function snapshot(id: string, database: string, schema: string): Promise<SchemaSnapshot> {
		const schemas = await catalog.schemas(id, database);
		if (!schemas.includes(schema))
			throw new Error(`Schema ${schema} was not found in ${database}.`);
		const relations = await catalog.relations(id, database, schema, true);
		return {
			relations: await Promise.all(
				relations.map(async (relation) => ({
					relation,
					columns: await api.listColumns(id, database, schema, relation.name)
				}))
			)
		};
	}
	async function compare() {
		busy = true;
		error = '';
		differences = null;
		try {
			const [left, right] = await Promise.all([
				snapshot(sourceId, sourceDatabase, sourceSchema),
				snapshot(targetId, targetDatabase, targetSchema)
			]);
			differences = compareSchemas(left, right);
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-2xl">
		<Dialog.Header
			><Dialog.Title>Compare schemas</Dialog.Title><Dialog.Description
				>Compare relation types and columns. Constraints, indexes and object definitions are not
				included.</Dialog.Description
			></Dialog.Header
		>
		<div class="grid gap-3 text-sm">
			<label
				>Source: {sourceDatabase}
				<Input
					aria-label="Source schema"
					bind:value={sourceSchema}
					placeholder="Source schema"
				/></label
			>
			<label
				>Target connection <select
					class="w-full rounded-md border bg-background p-2"
					bind:value={targetId}
					onchange={() => {
						targetDatabase = connections.get(targetId)?.database ?? '';
					}}
				>
					{#each connections.list as connection (connection.id)}<option value={connection.id}
							>{connection.name}</option
						>{/each}
				</select></label
			>
			<label>Target database <Input bind:value={targetDatabase} /></label>
			<label>Target schema <Input bind:value={targetSchema} /></label>
			<Button
				onclick={compare}
				disabled={busy ||
					!targetId ||
					!targetDatabase.trim() ||
					!sourceSchema.trim() ||
					!targetSchema.trim()}>{busy ? 'Comparing…' : 'Compare'}</Button
			>
			{#if error}<p role="alert" class="text-destructive">{error}</p>{/if}
			{#if differences !== null}<div class="max-h-64 overflow-auto" aria-live="polite">
					{#each differences as difference (difference)}<div class="border-b py-1">
							<strong>{difference.object}</strong>: {difference.detail}
						</div>{:else}<p>No differences in relation types or columns.</p>{/each}
				</div>{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>
