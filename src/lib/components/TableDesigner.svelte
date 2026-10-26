<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { api, errorMessage, type Relation, type Column } from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { confirmation } from '#lib/state/confirm.svelte.js';
	import {
		buildRequest,
		renameDraftColumn,
		destructive,
		structureApi,
		type TableStructure,
		type StructureColumn,
		type StructureConstraint
	} from '#lib/structure.js';

	interface Props {
		connectionId: string;
		database: string;
		schema: string;
		table?: string;
		onApplied?: (name: string) => void;
		onDirtyChange?: (count: number) => void;
		onBusyChange?: (busy: boolean) => void;
	}
	let { connectionId, database, schema, table, onApplied, onDirtyChange, onBusyChange }: Props =
		$props();
	let original = $state<TableStructure | null>(null);
	let draft = $state<TableStructure>({
		schema: untrack(() => schema),
		name: '',
		comment: null,
		kind: 'r',
		columns: [],
		constraints: [],
		indexes: []
	});
	let columnNames = $state<(string | null)[]>([]);
	let reloadRequired = $state(false);
	let loading = $state(untrack(() => !!table)),
		busy = $state(false),
		error = $state(''),
		sql = $state<string[]>([]),
		previewKey = $state(''),
		schemas = $state<string[]>([]),
		relations = $state<Record<string, Relation[]>>({}),
		refColumns = $state<Record<string, Column[]>>({});
	let section = $state<'columns' | 'constraints' | 'indexes'>('columns');
	let request = $derived(buildRequest(schema, original, draft, columnNames));
	let dirty = $derived(
		reloadRequired
			? 0
			: table ||
				  draft.name ||
				  draft.comment ||
				  draft.columns.length ||
				  draft.constraints.length ||
				  draft.indexes.length
				? request.changes.length
				: 0
	);
	let readOnly = $derived(
		connections.get(connectionId)?.readOnly !== false ||
			(!!table && !original) ||
			(!!original && original.kind !== 'r') ||
			reloadRequired
	);
	let requestKey = $derived(JSON.stringify(request));
	$effect(() => {
		onDirtyChange?.(dirty);
	});
	$effect(() => {
		onBusyChange?.(busy || loading);
	});
	async function load(name: string) {
		const loaded = await structureApi.load(connectionId, database, schema, name);
		original = loaded;
		draft = structuredClone(loaded);
		columnNames = draft.columns.map((c) => c.name);
	}
	onMount(async () => {
		try {
			if (table) await load(table);
			schemas = await api.listSchemas(connectionId, database);
			relations[schema] = await api.listRelations(connectionId, database, schema);
			for (const c of draft.constraints)
				if (c.refSchema && !relations[c.refSchema])
					relations[c.refSchema] = await api.listRelations(connectionId, database, c.refSchema);
			for (const c of draft.constraints)
				if (c.kind === 'foreignKey' && c.refTable) await references(c, false);
		} catch (e) {
			error = errorMessage(e);
		} finally {
			loading = false;
		}
	});
	function changed() {
		sql = [];
		previewKey = '';
		error = '';
	}
	function addColumn() {
		columnNames.push(null);
		draft.columns.push({
			name: '',
			dataType: 'text',
			nullable: true,
			default: null,
			identity: '',
			comment: null,
			generated: false
		});
		changed();
	}
	function renameColumn(column: StructureColumn, name: string) {
		changed();
		try {
			renameDraftColumn(draft, column, name);
		} catch (e) {
			error = errorMessage(e);
		}
	}
	function addConstraint() {
		draft.constraints.push({
			name: '',
			kind: 'unique',
			columns: [],
			refSchema: schema,
			refTable: null,
			refColumns: [],
			onUpdate: null,
			onDelete: null,
			expression: null,
			definition: ''
		});
		changed();
	}
	function addIndex() {
		draft.indexes.push({
			name: '',
			columns: [],
			unique: false,
			method: 'btree',
			definition: '',
			managed: false
		});
		changed();
	}
	function toggle(items: string[], name: string, checked: boolean) {
		return checked ? [...items, name] : items.filter((i) => i !== name);
	}
	function referenceOptions(c: StructureConstraint): { name: string }[] {
		return (c.refSchema || schema) === schema && c.refTable === draft.name && !original
			? draft.columns
			: (refColumns[`${c.refSchema || schema}.${c.refTable ?? ''}`] ?? []);
	}
	let checkColumn = $state(''),
		checkOperator = $state('>='),
		checkValue = $state('0');
	function setSimpleCheck(c: StructureConstraint) {
		if (checkColumn && /^-?\d+(?:\.\d+)?$/.test(checkValue))
			c.expression = `"${checkColumn.replaceAll('"', '""')}" ${checkOperator} ${checkValue}`;
		else error = 'Choose a column and enter a number.';
	}

	async function references(c: StructureConstraint, clear = true) {
		if (clear) c.refColumns = [];
		if (!c.refTable) return;
		if ((c.refSchema || schema) === schema && c.refTable === draft.name && !original) {
			refColumns[`${schema}.${c.refTable}`] = draft.columns.map((c) => ({
				name: c.name,
				dataType: c.dataType,
				notNull: !c.nullable,
				default: c.default,
				primaryKey: false
			}));
			return;
		}
		try {
			refColumns[`${c.refSchema || schema}.${c.refTable}`] = await api.listColumns(
				connectionId,
				database,
				c.refSchema || schema,
				c.refTable
			);
		} catch (e) {
			error = errorMessage(e);
		}
	}
	async function preview() {
		busy = true;
		error = '';
		try {
			const key = requestKey;
			sql = await structureApi.preview(request);
			previewKey = key;
		} catch (e) {
			error = errorMessage(e);
			previewKey = '';
		} finally {
			busy = false;
		}
	}
	async function apply() {
		if (busy || !previewKey || previewKey !== requestKey) return;
		busy = true;
		error = '';
		try {
			const current = JSON.parse(requestKey) as typeof request;
			sql = await structureApi.preview(current);
			if (
				destructive(current.changes) &&
				!(await confirmation.ask({
					title: 'Apply destructive changes?',
					description:
						'These changes can remove data or database objects. Review the SQL before applying.',
					confirmLabel: 'Apply changes',
					destructive: true
				}))
			)
				return;
			const name = await structureApi.apply(connectionId, database, current);
			try {
				await load(name);
			} catch (e) {
				reloadRequired = true;
				original = null;
				draft = {
					schema,
					name,
					comment: null,
					kind: 'r',
					columns: [],
					constraints: [],
					indexes: []
				};
				columnNames = [];
				error = `Changes applied, but reloading failed: ${errorMessage(e)}`;
			}
			sql = [];
			previewKey = '';
			onDirtyChange?.(0);
			onApplied?.(name);
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
		}
	}

	async function reload() {
		if (
			dirty &&
			!(await confirmation.ask({
				title: 'Discard pending changes?',
				description: 'Reloading replaces the current draft.',
				confirmLabel: 'Discard and reload',
				destructive: true
			}))
		)
			return;
		busy = true;
		try {
			await load(reloadRequired ? draft.name : (original?.name ?? table ?? draft.name));
			reloadRequired = false;
			changed();
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
		}
	}

	function discard() {
		if (original) draft = structuredClone($state.snapshot(original));
		else
			draft = {
				schema,
				name: '',
				comment: null,
				kind: 'r',
				columns: [],
				constraints: [],
				indexes: []
			};
		columnNames = original ? draft.columns.map((c) => c.name) : [];
		changed();
	}
</script>

<div class="flex h-full min-h-0 flex-col text-sm">
	{#if loading}<div class="p-4 text-muted-foreground">Loading structure…</div>{:else}
		<div class="flex flex-wrap items-center gap-2 border-b p-3">
			<strong>Table designer</strong><span class="text-muted-foreground">{schema}</span>
			<span class="flex-1"></span>{#if table || reloadRequired}<Button
					variant="outline"
					size="sm"
					disabled={busy}
					onclick={reload}>Reload</Button
				>{/if}<span class="text-muted-foreground">{dirty} pending</span>
			<Button variant="outline" size="sm" disabled={!dirty || busy || readOnly} onclick={discard}
				>Discard</Button
			>
			<Button variant="outline" size="sm" disabled={!dirty || busy || readOnly} onclick={preview}
				>Preview SQL</Button
			>
			<Button
				size="sm"
				disabled={!dirty || busy || readOnly || !previewKey || previewKey !== requestKey}
				onclick={apply}>{busy ? 'Working…' : 'Apply'}</Button
			>
		</div>
		{#if readOnly}<p class="border-b px-3 py-2 text-muted-foreground">
				This table is read-only.
			</p>{/if}
		{#if error}<p role="alert" class="border-b px-3 py-2 text-destructive">{error}</p>{/if}
		<div class="min-h-0 flex-1 overflow-auto p-4">
			<nav class="flex gap-1 border-b" aria-label="Structure sections">
				{#each ['columns', 'constraints', 'indexes'] as item (item)}<button
						type="button"
						class:active={section === item}
						class="section"
						onclick={() => (section = item as typeof section)}>{item}</button
					>{/each}
			</nav>
			<fieldset
				disabled={!!readOnly || busy}
				class="space-y-4 disabled:opacity-60"
				onchange={changed}
				oninput={changed}
			>
				<div class="grid gap-3 sm:grid-cols-2">
					<label class="space-y-1"
						>Table name<input
							class="field"
							value={draft.name}
							oninput={(e) => {
								const old = draft.name;
								draft.name = e.currentTarget.value;
								for (const c of draft.constraints)
									if ((c.refSchema ?? schema) === schema && c.refTable === old)
										c.refTable = draft.name;
							}}
							placeholder="Table name"
						/></label
					><label class="space-y-1"
						>Comment<input
							class="field"
							value={draft.comment ?? ''}
							oninput={(e) => (draft.comment = e.currentTarget.value || null)}
						/></label
					>
				</div>

				{#if section === 'columns'}
					{#each draft.columns as column, i (column)}
						<div class="card">
							<div class="grid gap-2 sm:grid-cols-3">
								<label
									>Name<input
										class="field"
										value={column.name}
										onchange={(e) => {
											e.stopPropagation();
											renameColumn(column, e.currentTarget.value);
											e.currentTarget.value = column.name;
										}}
										disabled={column.generated}
									/></label
								>
								<label
									>Data type<input
										class="field"
										bind:value={column.dataType}
										list="types"
										disabled={column.generated}
									/></label
								>
								<label
									>Default expression<input
										class="field"
										value={column.default ?? ''}
										oninput={(e) => (column.default = e.currentTarget.value || null)}
										disabled={column.generated}
										placeholder="None"
									/></label
								>
								<label
									>Default preset<select
										class="field"
										value=""
										disabled={column.generated}
										onchange={(e) => {
											const value = e.currentTarget.value;
											if (value === 'none') column.default = null;
											else if (value === 'text') column.default = "''";
											else if (value) column.default = value;
											e.currentTarget.value = '';
										}}
										><option value="">Choose a preset</option><option value="none"
											>No default</option
										><option value="text">Empty text</option><option value="0">Zero</option><option
											value="true">True</option
										><option value="false">False</option><option value="CURRENT_TIMESTAMP"
											>Current timestamp</option
										></select
									></label
								>
								<label
									>Identity<select
										class="field"
										bind:value={column.identity}
										onchange={() => {
											if (column.identity) {
												column.nullable = false;
												column.default = null;
											}
										}}
										disabled={column.generated}
										><option value="">None</option><option value="always">Always</option><option
											value="byDefault">By default</option
										></select
									></label
								>
								<label
									>Comment<input
										class="field"
										value={column.comment ?? ''}
										oninput={(e) => (column.comment = e.currentTarget.value || null)}
										disabled={column.generated}
									/></label
								>
								<div class="flex items-end justify-between gap-2">
									<label class="flex items-center gap-2 pb-2"
										><input
											type="checkbox"
											bind:checked={column.nullable}
											disabled={column.generated}
										/> Nullable</label
									><Button
										type="button"
										variant="ghost"
										size="sm"
										onclick={() => {
											draft.columns.splice(i, 1);
											columnNames.splice(i, 1);
											changed();
										}}>Remove</Button
									>
								</div>
							</div>
							{#if column.generated}<p class="mt-2 text-muted-foreground">
									Generated column (read-only)
								</p>{/if}
						</div>
					{/each}
					<datalist id="types"
						><option value="text"></option><option value="integer"></option><option value="bigint"
						></option><option value="boolean"></option><option value="numeric"></option><option
							value="uuid"
						></option><option value="timestamp with time zone"></option><option value="jsonb"
						></option></datalist
					>
					<Button type="button" variant="outline" size="sm" onclick={addColumn}>Add column</Button>
				{:else if section === 'constraints'}
					{#each draft.constraints as c, i (c)}
						<div class="card space-y-3">
							{#if c.kind === 'other'}<div class="flex justify-between">
									<span>{c.name}: {c.definition}</span>
								</div>
								<p class="text-muted-foreground">This constraint type is read-only.</p>{:else}
								<div class="grid gap-2 sm:grid-cols-2">
									<label>Name<input class="field" bind:value={c.name} /></label><label
										>Type<select class="field" bind:value={c.kind}
											><option value="primaryKey">Primary key</option><option value="unique"
												>Unique</option
											><option value="foreignKey">Foreign key</option><option value="check"
												>Check</option
											></select
										></label
									>
								</div>
								{#if c.kind === 'check'}<div class="grid gap-2 sm:grid-cols-3">
										<label
											>Column<select class="field" bind:value={checkColumn}
												><option value="">Select column</option
												>{#each draft.columns as col (col)}<option value={col.name}
														>{col.name}</option
													>{/each}</select
											></label
										><label
											>Operator<select class="field" bind:value={checkOperator}
												><option>&gt;=</option><option>&gt;</option><option>&lt;=</option><option
													>&lt;</option
												><option>=</option><option>&lt;&gt;</option></select
											></label
										><label>Number<input class="field" bind:value={checkValue} /></label>
									</div>
									<Button
										type="button"
										variant="outline"
										size="sm"
										onclick={() => setSimpleCheck(c)}>Use condition</Button
									><label
										>Check expression<input
											class="field"
											value={c.expression ?? ''}
											oninput={(e) => (c.expression = e.currentTarget.value || null)}
											placeholder="price > 0"
										/></label
									>{:else}
									<div>
										Columns <span class="text-muted-foreground"
											>(selected order: {c.columns.join(' → ') || 'none'})</span
										>
										<div class="flex flex-wrap gap-3">
											{#each draft.columns as col (col)}<label class="flex items-center gap-1"
													><input
														type="checkbox"
														checked={c.columns.includes(col.name)}
														onchange={(e) =>
															(c.columns = toggle(c.columns, col.name, e.currentTarget.checked))}
													/>{col.name}</label
												>{/each}
										</div>
									</div>
									{#if c.kind === 'foreignKey'}<div class="grid gap-2 sm:grid-cols-2">
											<label
												>Referenced schema<select
													class="field"
													value={c.refSchema ?? schema}
													onchange={async (e) => {
														c.refSchema = e.currentTarget.value;
														c.refTable = null;
														c.refColumns = [];
														try {
															if (!relations[c.refSchema])
																relations[c.refSchema] = await api.listRelations(
																	connectionId,
																	database,
																	c.refSchema
																);
														} catch (e) {
															error = errorMessage(e);
														}
													}}
													>{#each schemas as refSchema (refSchema)}<option value={refSchema}
															>{refSchema}</option
														>{/each}</select
												></label
											><label
												>Referenced table<select
													class="field"
													value={c.refTable ?? ''}
													onchange={(e) => {
														c.refTable = e.currentTarget.value || null;
														void references(c);
													}}
													><option value="">Select table</option
													>{#if (c.refSchema || schema) === schema && draft.name && !(relations[schema] ?? []).some((r) => r.name === draft.name)}<option
															value={draft.name}>{draft.name} (this table)</option
														>{/if}{#each relations[c.refSchema || schema] ?? [] as rel (rel.name)}<option
															value={rel.name}>{rel.name}</option
														>{/each}</select
												></label
											>
										</div>
										<div>
											Referenced columns <span class="text-muted-foreground"
												>(selected order: {c.refColumns.join(' → ') || 'none'})</span
											>
											<p class="text-muted-foreground">
												Mapping: {c.columns
													.map((name, i) => `${name} → ${c.refColumns[i] || '…'}`)
													.join(', ') || 'none'}
											</p>
											<div class="flex flex-wrap gap-3">
												{#each referenceOptions(c) as col (col)}<label
														class="flex items-center gap-1"
														><input
															type="checkbox"
															checked={c.refColumns.includes(col.name)}
															onchange={(e) =>
																(c.refColumns = toggle(
																	c.refColumns,
																	col.name,
																	e.currentTarget.checked
																))}
														/>{col.name}</label
													>{/each}
											</div>
										</div>
										<div class="grid gap-2 sm:grid-cols-2">
											{#each ['onUpdate', 'onDelete'] as action (action)}<label
													>{action === 'onUpdate' ? 'On update' : 'On delete'}<select
														class="field"
														value={c[action as 'onUpdate' | 'onDelete'] ?? ''}
														onchange={(e) =>
															(c[action as 'onUpdate' | 'onDelete'] =
																e.currentTarget.value || null)}
														><option value="">Default</option><option>NO ACTION</option><option
															>RESTRICT</option
														><option>CASCADE</option><option>SET NULL</option><option
															>SET DEFAULT</option
														></select
													></label
												>{/each}
										</div>
									{/if}
								{/if}<Button
									type="button"
									variant="ghost"
									size="sm"
									onclick={() => {
										draft.constraints.splice(i, 1);
										changed();
									}}>Remove</Button
								>{/if}
						</div>
					{/each}<Button type="button" variant="outline" size="sm" onclick={addConstraint}
						>Add constraint</Button
					>
				{:else}
					{#each draft.indexes as idx, i (idx)}<div class="card space-y-3">
							{#if idx.managed}<p>{idx.name}: {idx.definition}</p>
								<p class="text-muted-foreground">Managed or complex index (read-only)</p>{:else}<div
									class="grid gap-2 sm:grid-cols-2"
								>
									<label>Name<input class="field" bind:value={idx.name} /></label><label
										>Method<select class="field" bind:value={idx.method}
											><option>btree</option><option>hash</option><option>gin</option><option
												>gist</option
											><option>brin</option><option>spgist</option></select
										></label
									>
								</div>
								<div>
									Columns
									<div class="flex flex-wrap gap-3">
										{#each draft.columns as col (col)}<label class="flex items-center gap-1"
												><input
													type="checkbox"
													checked={idx.columns.includes(col.name)}
													onchange={(e) =>
														(idx.columns = toggle(idx.columns, col.name, e.currentTarget.checked))}
												/>{col.name}</label
											>{/each}
									</div>
								</div>
								<label class="flex items-center gap-2"
									><input type="checkbox" bind:checked={idx.unique} /> Unique</label
								><Button
									type="button"
									variant="ghost"
									size="sm"
									onclick={() => {
										draft.indexes.splice(i, 1);
										changed();
									}}>Remove</Button
								>{/if}
						</div>{/each}<Button type="button" variant="outline" size="sm" onclick={addIndex}
						>Add index</Button
					>
				{/if}
			</fieldset>
			{#if sql.length}<div class="mt-5 space-y-2">
					<h3 class="font-semibold">SQL preview</h3>
					<pre class="overflow-auto rounded-md border bg-muted p-3 text-xs">{sql.join(';\n\n') +
							';'}</pre>
				</div>{/if}
		</div>{/if}
</div>

<style>
	.field {
		display: block;
		width: 100%;
		border: 1px solid var(--border);
		border-radius: 0.375rem;
		padding: 0.4rem 0.55rem;
		background: var(--background);
		margin-top: 0.25rem;
	}
	.card {
		border: 1px solid var(--border);
		border-radius: 0.5rem;
		padding: 0.75rem;
	}
	.section {
		padding: 0.5rem 0.75rem;
		text-transform: capitalize;
		color: var(--muted-foreground);
		border-bottom: 2px solid transparent;
	}
	.section.active {
		color: var(--foreground);
		border-color: var(--primary);
	}
</style>
