<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import type { Attachment } from 'svelte/attachments';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import LinkIcon from '@lucide/svelte/icons/link';
	import ListChecksIcon from '@lucide/svelte/icons/list-checks';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import LockIcon from '@lucide/svelte/icons/lock';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import RefreshIcon from '@lucide/svelte/icons/refresh-cw';
	import FingerprintIcon from '@lucide/svelte/icons/fingerprint';
	import HashIcon from '@lucide/svelte/icons/hash';
	import Table2Icon from '@lucide/svelte/icons/table-2';
	import TrashIcon from '@lucide/svelte/icons/trash';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
	import UndoIcon from '@lucide/svelte/icons/undo-2';
	import { api, errorMessage, type Relation, type Column } from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { highlightSql } from '#lib/highlight.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { confirmation } from '#lib/state/confirm.svelte.js';
	import {
		buildRequest,
		changeStatus,
		columnRenames,
		defaultPresets,
		matchReferences,
		destructive,
		namedStatus,
		primaryKeyColumns,
		removedItems,
		renameDraftColumn,
		restoreColumn,
		restored,
		structureApi,
		suggestName,
		supportsIdentity,
		toggleForeignKeyColumn,
		togglePrimaryKey,
		type ItemStatus,
		type StructureColumn,
		type StructureConstraint,
		type StructureIndex,
		type TableStructure
	} from '#lib/structure.js';
	import CheckBuilder from './CheckBuilder.svelte';

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

	const empty = (): TableStructure => ({
		schema,
		name: '',
		comment: null,
		kind: 'r',
		columns: [],
		constraints: [],
		indexes: []
	});
	let original = $state<TableStructure | null>(null);
	let draft = $state<TableStructure>(untrack(empty));
	/** Original name of each draft column by position; null for columns added in this draft. */
	let columnNames = $state<(string | null)[]>([]);
	let reloadRequired = $state(false);
	let loading = $state(untrack(() => !!table)),
		busy = $state(false),
		error = $state(''),
		sql = $state<string[]>([]),
		previewKey = $state(''),
		reviewing = $state(false),
		schemas = $state<string[]>([]),
		relations = $state<Record<string, Relation[]>>({}),
		refColumns = $state<Record<string, Column[]>>({});
	let section = $state<'columns' | 'constraints' | 'indexes'>('columns');
	/** Constraints and indexes whose name still follows their columns until the user types one. */
	const autoNamed = new WeakSet<object>();
	let focusNext: object | null = null;

	let request = $derived(buildRequest(schema, original, draft, columnNames));
	let requestKey = $derived(JSON.stringify(request));
	let status = $derived(changeStatus(request));
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
	let readOnlyReason = $derived(
		reloadRequired
			? 'Reload to keep editing.'
			: connections.get(connectionId)?.readOnly !== false
				? 'Read-only connection.'
				: table && !original
					? 'The structure could not be loaded.'
					: original && original.kind !== 'r'
						? 'Only plain tables can be edited here.'
						: ''
	);
	let locked = $derived(!!readOnlyReason || busy);
	let keyColumns = $derived(primaryKeyColumns(draft));
	let removedColumns = $derived(
		(original?.columns ?? []).filter((c) => status.droppedColumns.includes(c.name))
	);
	let removedConstraints = $derived(
		removedItems(original?.constraints, draft.constraints, status.droppedConstraints)
	);
	let removedIndexes = $derived(
		removedItems(original?.indexes, draft.indexes, status.droppedIndexes)
	);
	let columnList = $derived(draft.columns.map((c) => c.name).filter(Boolean));
	let destroys = $derived(destructive(request.changes));
	let counts = $derived({
		columns: draft.columns.length,
		constraints: draft.constraints.length,
		indexes: draft.indexes.length
	});
	const TYPES = [
		'text',
		'varchar(255)',
		'integer',
		'bigint',
		'smallint',
		'numeric',
		'numeric(12,2)',
		'real',
		'double precision',
		'boolean',
		'uuid',
		'date',
		'time',
		'timestamp',
		'timestamp with time zone',
		'interval',
		'jsonb',
		'json',
		'bytea',
		'inet',
		'text[]'
	];
	const IDENTITY = { '': 'None', always: 'Always', byDefault: 'By default' } as const;
	const SECTIONS = ['columns', 'constraints', 'indexes'] as const;
	const FK_ACTIONS = ['onUpdate', 'onDelete'] as const;
	const NEW_KINDS = ['primaryKey', 'unique', 'foreignKey', 'check'] as const;
	const ACTIONS = ['NO ACTION', 'RESTRICT', 'CASCADE', 'SET NULL', 'SET DEFAULT'];
	const METHODS = ['btree', 'hash', 'gin', 'gist', 'brin', 'spgist'];
	const KINDS = {
		primaryKey: { label: 'Primary key', short: 'PK', icon: KeyRoundIcon },
		unique: { label: 'Unique', short: 'Unique', icon: FingerprintIcon },
		foreignKey: { label: 'Foreign key', short: 'FK', icon: LinkIcon },
		check: { label: 'Check', short: 'Check', icon: ListChecksIcon },
		other: { label: 'Other', short: 'Other', icon: LockIcon }
	} as const;

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
				if (c.kind === 'foreignKey' && c.refTable) await references(c);
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
	const focusWhenNew =
		(item: object): Attachment<HTMLInputElement> =>
		(node) => {
			if (focusNext !== item) return;
			focusNext = null;
			node.focus();
		};
	const focusOnCreate: Attachment<HTMLInputElement> = (node) => {
		if (!table) node.focus();
	};

	function columnStatus(i: number): ItemStatus {
		if (!original) return 'same';
		const origin = columnNames[i];
		if (!origin) return 'new';
		return status.alteredColumns.has(origin) ? 'changed' : 'same';
	}
	function constraintStatus(c: StructureConstraint): ItemStatus {
		return original ? namedStatus(c.name, original.constraints, status.addedConstraints) : 'same';
	}
	function indexStatus(idx: StructureIndex): ItemStatus {
		return original ? namedStatus(idx.name, original.indexes, status.addedIndexes) : 'same';
	}

	function renameTable(name: string) {
		const old = draft.name;
		draft.name = name;
		for (const c of draft.constraints) {
			if ((c.refSchema ?? schema) === schema && c.refTable === old) c.refTable = name;
			if (autoNamed.has(c)) c.name = suggestName(draft, nameKind(c.kind), c.columns, c);
		}
		for (const idx of draft.indexes)
			if (autoNamed.has(idx)) idx.name = suggestName(draft, 'index', idx.columns, idx);
	}
	const nameKind = (kind: StructureConstraint['kind']) => (kind === 'other' ? 'check' : kind);

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
		focusNext = draft.columns.at(-1)!;
		section = 'columns';
		changed();
	}
	function removeColumn(i: number) {
		const name = draft.columns[i].name;
		draft.columns.splice(i, 1);
		columnNames.splice(i, 1);
		for (const c of draft.constraints) {
			const at = c.columns.indexOf(name);
			if (at < 0) continue;
			c.columns = c.columns.filter((_, n) => n !== at);
			if (c.kind === 'foreignKey') c.refColumns = c.refColumns.filter((_, n) => n !== at);
		}
		draft.constraints = draft.constraints.filter(
			(c) => c.kind === 'other' || c.kind === 'check' || c.columns.length
		);
		for (const idx of draft.indexes) idx.columns = idx.columns.filter((n) => n !== name);
		draft.indexes = draft.indexes.filter((idx) => idx.managed || idx.columns.length);
		changed();
	}
	function renameColumn(column: StructureColumn, name: string) {
		changed();
		try {
			renameDraftColumn(draft, column, name.trim());
		} catch (e) {
			error = errorMessage(e);
		}
	}
	function restore(fn: () => void) {
		changed();
		try {
			fn();
		} catch (e) {
			error = errorMessage(e);
		}
	}
	function setIdentity(column: StructureColumn, value: string) {
		column.identity = value as StructureColumn['identity'];
		if (column.identity) {
			column.nullable = false;
			column.default = null;
		}
		changed();
	}

	function addConstraint(kind: Exclude<StructureConstraint['kind'], 'other'>) {
		draft.constraints.push({
			name: suggestName(draft, kind, []),
			kind,
			columns: [],
			refSchema: kind === 'foreignKey' ? schema : null,
			refTable: null,
			refColumns: [],
			onUpdate: null,
			onDelete: null,
			expression: null,
			definition: ''
		});
		autoNamed.add(draft.constraints.at(-1)!);
		section = 'constraints';
		changed();
	}
	function toggleConstraintColumn(c: StructureConstraint, column: string) {
		if (c.kind === 'foreignKey') toggleForeignKeyColumn(c, column, referenceOptions(c));
		else
			c.columns = c.columns.includes(column)
				? c.columns.filter((n) => n !== column)
				: [...c.columns, column];
		if (c.kind === 'primaryKey')
			for (const col of draft.columns) if (c.columns.includes(col.name)) col.nullable = false;
		if (autoNamed.has(c)) c.name = suggestName(draft, nameKind(c.kind), c.columns, c);
		changed();
	}
	function addIndex() {
		draft.indexes.push({
			name: suggestName(draft, 'index', []),
			columns: [],
			unique: false,
			method: 'btree',
			definition: '',
			managed: false
		});
		autoNamed.add(draft.indexes.at(-1)!);
		section = 'indexes';
		changed();
	}
	function toggleIndexColumn(idx: StructureIndex, column: string) {
		idx.columns = idx.columns.includes(column)
			? idx.columns.filter((n) => n !== column)
			: [...idx.columns, column];
		if (autoNamed.has(idx)) idx.name = suggestName(draft, 'index', idx.columns, idx);
		changed();
	}

	function referenceOptions(c: StructureConstraint): { name: string; primaryKey?: boolean }[] {
		return (c.refSchema || schema) === schema && c.refTable === draft.name && !original
			? draft.columns.map((col) => ({ name: col.name, primaryKey: keyColumns.includes(col.name) }))
			: (refColumns[`${c.refSchema || schema}.${c.refTable ?? ''}`] ?? []);
	}
	async function references(c: StructureConstraint) {
		if (!c.refTable) return;
		if ((c.refSchema || schema) === schema && c.refTable === draft.name && !original) return;
		const key = `${c.refSchema || schema}.${c.refTable}`;
		if (refColumns[key]) return;
		try {
			refColumns[key] = await api.listColumns(
				connectionId,
				database,
				c.refSchema || schema,
				c.refTable
			);
		} catch (e) {
			error = errorMessage(e);
		}
	}
	async function setRefSchema(c: StructureConstraint, value: string) {
		c.refSchema = value;
		c.refTable = null;
		c.refColumns = c.columns.map(() => '');
		changed();
		try {
			if (!relations[value])
				relations[value] = await api.listRelations(connectionId, database, value);
		} catch (e) {
			error = errorMessage(e);
		}
	}
	async function setRefTable(c: StructureConstraint, value: string) {
		c.refTable = value || null;
		changed();
		await references(c);
		c.refColumns = matchReferences(c.columns, referenceOptions(c));
	}
	function refTables(c: StructureConstraint): string[] {
		const names = (relations[c.refSchema || schema] ?? []).map((r) => r.name);
		if ((c.refSchema || schema) === schema && draft.name && !names.includes(draft.name))
			names.unshift(draft.name);
		return names;
	}

	async function review() {
		if (busy || !dirty) return;
		busy = true;
		error = '';
		try {
			const key = requestKey;
			sql = await structureApi.preview(request);
			previewKey = key;
			reviewing = true;
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
			const name = await structureApi.apply(connectionId, database, current);
			reviewing = false;
			try {
				await load(name);
			} catch (e) {
				reloadRequired = true;
				original = null;
				draft = { ...empty(), name };
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
		draft = original ? structuredClone($state.snapshot(original)) : empty();
		columnNames = original ? draft.columns.map((c) => c.name) : [];
		changed();
	}

	const rowTone: Record<ItemStatus, string> = {
		same: '',
		new: 'bg-ok-soft/60',
		changed: 'bg-signal-soft/50'
	};
	const gutterTone: Record<ItemStatus, string> = {
		same: 'text-muted-foreground',
		new: 'bg-ok-soft text-ok',
		changed: 'bg-signal-soft text-signal-text'
	};
	const statusLabel: Record<ItemStatus, string> = { same: '', new: 'New', changed: 'Changed' };
</script>

{#snippet columnChips(selected: string[], toggle: (name: string) => void, ordered = false)}
	<div class="flex flex-wrap gap-1.5">
		{#each columnList as name (name)}
			{@const at = selected.indexOf(name)}
			<button
				type="button"
				class="chip-toggle"
				aria-pressed={at >= 0}
				disabled={locked}
				onclick={() => toggle(name)}
			>
				{#if ordered && at >= 0 && selected.length > 1}<span class="measured order">{at + 1}</span
					>{/if}{name}
			</button>
		{:else}
			<span class="text-xs text-muted-foreground">Add columns first.</span>
		{/each}
	</div>
{/snippet}

{#snippet statusChip(s: ItemStatus)}
	{#if s !== 'same'}<span class="chip {s === 'new' ? 'chip-ok' : 'chip-signal'}"
			>{statusLabel[s]}</span
		>{/if}
{/snippet}

{#snippet removedRow(label: string, detail: string, onrestore: () => void)}
	<div
		class="flex items-center gap-3 rounded-lg border border-dashed border-[color-mix(in_oklab,var(--color-bad),transparent_60%)] bg-bad-soft px-3 py-1.5 text-xs"
	>
		<TrashIcon class="size-3.5 text-bad" />
		<span class="measured text-muted-foreground line-through">{label}</span>
		<span class="measured truncate text-muted-foreground">{detail}</span>
		<span class="flex-1"></span>
		<span class="text-bad">Will be dropped</span>
		<Button size="xs" variant="ghost" disabled={locked} onclick={onrestore}
			><UndoIcon /> Restore</Button
		>
	</div>
{/snippet}

<div class="flex h-full min-h-0 flex-col bg-background text-sm">
	{#if loading}
		<div class="flex flex-1 items-center justify-center gap-2 text-muted-foreground">
			<LoaderIcon class="size-4 animate-spin" /> Loading structure…
		</div>
	{:else}
		<header class="flex shrink-0 flex-col gap-2 border-b bg-chrome px-3 pt-2.5">
			<div class="flex items-center gap-2">
				<Table2Icon class="size-4 shrink-0 text-muted-foreground" />
				<div class="flex min-w-0 items-center">
					<span class="measured text-muted-foreground">{schema}.</span>
					<input
						class="title-field measured"
						value={draft.name}
						size={Math.max(draft.name.length, 10)}
						placeholder="table_name"
						aria-label="Table name"
						aria-invalid={!draft.name && dirty > 0}
						disabled={locked}
						oninput={(e) => {
							renameTable(e.currentTarget.value);
							changed();
						}}
						{@attach focusOnCreate}
					/>
				</div>
				<input
					class="cell min-w-0 flex-1 text-muted-foreground focus:text-foreground"
					value={draft.comment ?? ''}
					placeholder="Add a table comment"
					aria-label="Table comment"
					disabled={locked}
					oninput={(e) => {
						draft.comment = e.currentTarget.value || null;
						changed();
					}}
				/>
				{#if table || reloadRequired}
					<Button
						size="icon-sm"
						variant="ghost"
						disabled={busy}
						onclick={reload}
						title="Reload structure from the database"
						aria-label="Reload"><RefreshIcon /></Button
					>
				{/if}
			</div>
			<div class="-mb-px flex gap-1" role="tablist" aria-label="Structure sections">
				{#each SECTIONS as item (item)}
					<button
						type="button"
						role="tab"
						aria-selected={section === item}
						class="section-tab"
						onclick={() => (section = item)}
					>
						<span class="capitalize">{item}</span>
						<span class="measured text-xs text-muted-foreground">{counts[item]}</span>
					</button>
				{/each}
			</div>
		</header>

		{#if error}
			<p
				role="alert"
				class="flex shrink-0 items-start gap-2 border-b bg-bad-soft px-3 py-2 text-xs text-bad"
			>
				<TriangleAlertIcon class="mt-px size-3.5 shrink-0" />{error}
			</p>
		{/if}

		<div class="min-h-0 flex-1 overflow-auto">
			<fieldset disabled={locked} class="min-w-0" onchange={changed} oninput={changed}>
				{#if section === 'columns'}
					<table class="w-full min-w-[56rem] table-fixed border-collapse">
						<colgroup>
							<col class="w-9" />
							<col class="w-[18%]" />
							<col class="w-[16%]" />
							<col class="w-20" />
							<col class="w-[18%]" />
							<col class="w-32" />
							<col />
							<col class="w-10" />
						</colgroup>
						<thead class="sticky top-0 z-10 bg-grid-head text-left text-xs text-muted-foreground">
							<tr class="border-b">
								<th class="sr-only">Key</th>
								<th class="px-2 py-1.5 font-medium">Name</th>
								<th class="px-2 py-1.5 font-medium">Type</th>
								<th class="px-2 py-1.5 text-center font-medium">Not null</th>
								<th class="px-2 py-1.5 font-medium">Default</th>
								<th class="px-2 py-1.5 font-medium">Identity</th>
								<th class="px-2 py-1.5 font-medium">Comment</th>
								<th class="sr-only">Actions</th>
							</tr>
						</thead>
						<tbody>
							{#each draft.columns as column, i (column)}
								{@const s = columnStatus(i)}
								{@const isKey = keyColumns.includes(column.name)}
								{@const fixed = locked || column.generated}
								{@const presets = defaultPresets(column.dataType)}
								<tr class="group border-rule/70 border-b {rowTone[s]} hover:bg-hover/40">
									<td class="p-0 {gutterTone[s]}" title={statusLabel[s]}>
										<button
											type="button"
											class="key-toggle"
											aria-pressed={isKey}
											aria-label={isKey ? 'Remove from primary key' : 'Add to primary key'}
											title={isKey ? 'Primary key, click to remove' : 'Make primary key'}
											disabled={fixed || !column.name}
											onclick={() => {
												togglePrimaryKey(draft, column);
												changed();
											}}
										>
											{#if column.generated}<LockIcon class="size-3.5" />{:else}<KeyRoundIcon
													class="size-3.5"
												/>{/if}
										</button>
									</td>
									<td class="px-1 py-1">
										<input
											class="cell measured"
											value={column.name}
											placeholder="column_name"
											aria-label="Column name"
											aria-invalid={!column.name}
											disabled={fixed}
											onchange={(e) => {
												e.stopPropagation();
												renameColumn(column, e.currentTarget.value);
												e.currentTarget.value = column.name;
											}}
											{@attach focusWhenNew(column)}
										/>
									</td>
									<td class="px-1 py-1">
										<input
											class="cell measured"
											bind:value={column.dataType}
											list="designer-types"
											aria-label="Data type"
											disabled={fixed}
											onchange={() => {
												if (!supportsIdentity(column.dataType)) column.identity = '';
											}}
										/>
									</td>
									<td class="px-1 py-1 text-center">
										<Checkbox
											class="mx-auto"
											checked={!column.nullable}
											disabled={fixed || isKey || !!column.identity}
											title={isKey || column.identity
												? 'Keys and identity columns are always not null'
												: undefined}
											aria-label="Not null"
											onCheckedChange={(value) => {
												column.nullable = !value;
												changed();
											}}
										/>
									</td>
									<td class="px-1 py-1">
										<div class="flex items-center">
											<input
												class="cell measured rounded-r-none"
												value={column.default ?? ''}
												placeholder={column.identity ? 'identity' : 'none'}
												aria-label="Default expression"
												disabled={fixed || !!column.identity}
												oninput={(e) => (column.default = e.currentTarget.value || null)}
											/>
											<DropdownMenu.Root>
												<DropdownMenu.Trigger disabled={fixed || !!column.identity}>
													{#snippet child({ props })}
														<button
															{...props}
															type="button"
															class="preset-trigger"
															aria-label="Default presets"
															title="Common defaults"><ChevronDownIcon class="size-3.5" /></button
														>
													{/snippet}
												</DropdownMenu.Trigger>
												<DropdownMenu.Content align="end" class="w-48">
													{#each presets as preset (preset.label)}
														<DropdownMenu.Item
															onSelect={() => {
																column.default = preset.value;
																changed();
															}}
														>
															{preset.label}
															{#if preset.value}<span
																	class="measured ml-auto truncate text-xs text-muted-foreground"
																	>{preset.value}</span
																>{/if}
														</DropdownMenu.Item>
													{/each}
												</DropdownMenu.Content>
											</DropdownMenu.Root>
										</div>
									</td>
									<td class="px-1 py-1">
										<Select.Root
											type="single"
											value={column.identity || 'none'}
											onValueChange={(v) => setIdentity(column, v === 'none' ? '' : v)}
											disabled={fixed || (!column.identity && !supportsIdentity(column.dataType))}
										>
											<Select.Trigger
												size="sm"
												class="w-full border-transparent bg-transparent {column.identity
													? ''
													: 'text-muted-foreground'}"
												aria-label="Identity"
												title={supportsIdentity(column.dataType)
													? undefined
													: 'Identity needs an integer type'}
											>
												{IDENTITY[column.identity]}
											</Select.Trigger>
											<Select.Content>
												{#each Object.entries(IDENTITY) as [value, label] (value)}
													<Select.Item value={value || 'none'} {label} />
												{/each}
											</Select.Content>
										</Select.Root>
									</td>
									<td class="px-1 py-1">
										<input
											class="cell"
											value={column.comment ?? ''}
											aria-label="Column comment"
											disabled={fixed}
											placeholder={column.generated ? 'Generated column' : ''}
											oninput={(e) => (column.comment = e.currentTarget.value || null)}
										/>
									</td>
									<td class="px-1 py-1 text-right">
										<Button
											size="icon-xs"
											variant="ghost"
											class="opacity-50 group-hover:opacity-100 focus-visible:opacity-100"
											disabled={locked}
											title="Remove column"
											aria-label="Remove column {column.name}"
											onclick={() => removeColumn(i)}><TrashIcon /></Button
										>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
					<datalist id="designer-types">
						{#each TYPES as type (type)}<option value={type}></option>{/each}
					</datalist>
					<div class="space-y-1.5 p-3">
						{#each removedColumns as column (column.name)}
							{@render removedRow(column.name, column.dataType, () =>
								restore(() => restoreColumn(original!, draft, columnNames, column.name))
							)}
						{/each}
						{#if !draft.columns.length && !removedColumns.length}
							<p class="py-6 text-center text-muted-foreground">
								No columns yet. Add the first one to define the table.
							</p>
						{/if}
						<Button size="sm" variant="outline" disabled={locked} onclick={addColumn}
							><PlusIcon /> Add column</Button
						>
					</div>
				{:else if section === 'constraints'}
					<div class="space-y-2 p-3">
						{#each draft.constraints as c, i (c)}
							{@const s = constraintStatus(c)}
							{@const Icon = KINDS[c.kind].icon}
							<section class="item {s !== 'same' ? `item-${s}` : ''}">
								<div class="flex items-center gap-2">
									<span class="chip shrink-0" title={KINDS[c.kind].label}
										><Icon class="size-3" />{KINDS[c.kind].short}</span
									>
									{#if c.kind === 'other'}
										<span class="measured truncate">{c.name}</span>
										<span class="flex-1"></span>
										<span class="chip"><LockIcon class="size-3" /> Read-only</span>
									{:else}
										<input
											class="cell measured max-w-80"
											value={c.name}
											aria-label="Constraint name"
											aria-invalid={!c.name}
											oninput={(e) => {
												c.name = e.currentTarget.value;
												autoNamed.delete(c);
											}}
										/>
										<span class="flex-1"></span>
										{@render statusChip(s)}
										<Button
											size="icon-xs"
											variant="ghost"
											disabled={locked}
											title="Remove constraint"
											aria-label="Remove constraint {c.name}"
											onclick={() => {
												draft.constraints.splice(i, 1);
												changed();
											}}><TrashIcon /></Button
										>
									{/if}
								</div>
								{#if c.kind === 'other'}
									<p class="measured mt-2 text-xs break-all text-muted-foreground">
										{c.definition}
									</p>
								{:else if c.kind === 'check'}
									<div class="mt-2 space-y-2">
										<input
											class="field measured"
											value={c.expression ?? ''}
											placeholder="price > 0"
											aria-label="Check expression"
											aria-invalid={!c.expression}
											oninput={(e) => (c.expression = e.currentTarget.value || null)}
										/>
										<CheckBuilder
											columns={columnList}
											disabled={locked}
											onapply={(expression) => {
												c.expression = expression;
												changed();
											}}
										/>
									</div>
								{:else}
									<div class="mt-2 space-y-2">
										<div class="flex items-baseline gap-3">
											<span class="label">Columns</span>
											{@render columnChips(
												c.columns,
												(name) => toggleConstraintColumn(c, name),
												true
											)}
										</div>
										{#if c.kind === 'foreignKey'}
											<div class="flex flex-wrap items-center gap-2">
												<span class="label">References</span>
												<Select.Root
													type="single"
													value={c.refSchema ?? schema}
													onValueChange={(v) => setRefSchema(c, v)}
													disabled={locked}
												>
													<Select.Trigger size="sm" class="measured min-w-28" aria-label="Schema"
														>{c.refSchema ?? schema}</Select.Trigger
													>
													<Select.Content>
														{#each schemas as name (name)}<Select.Item
																value={name}
																label={name}
															/>{/each}
													</Select.Content>
												</Select.Root>
												<span class="text-muted-foreground">.</span>
												<Select.Root
													type="single"
													value={c.refTable ?? ''}
													onValueChange={(v) => setRefTable(c, v)}
													disabled={locked}
												>
													<Select.Trigger
														size="sm"
														class="measured min-w-40 {c.refTable ? '' : 'text-muted-foreground'}"
														aria-label="Referenced table"
														aria-invalid={!c.refTable}
														>{c.refTable ?? 'Choose a table'}</Select.Trigger
													>
													<Select.Content class="max-h-72">
														{#each refTables(c) as name (name)}<Select.Item
																value={name}
																label={name}
																>{name}{#if name === draft.name && (c.refSchema || schema) === schema}<span
																		class="text-muted-foreground">(this table)</span
																	>{/if}</Select.Item
															>{/each}
													</Select.Content>
												</Select.Root>
											</div>
											{#if c.columns.length && c.refTable}
												<div class="ml-[5.5rem] space-y-1">
													{#each c.columns as name, n (name)}
														<div class="flex items-center gap-2 text-xs">
															<span class="measured w-36 truncate">{name}</span>
															<span class="text-muted-foreground">→</span>
															<Select.Root
																type="single"
																value={c.refColumns[n] ?? ''}
																onValueChange={(v) => {
																	const next = [...c.refColumns];
																	next[n] = v;
																	c.refColumns = next;
																	changed();
																}}
																disabled={locked}
															>
																<Select.Trigger
																	size="sm"
																	class="measured min-w-40 {c.refColumns[n]
																		? ''
																		: 'text-muted-foreground'}"
																	aria-label="Referenced column for {name}"
																	aria-invalid={!c.refColumns[n]}
																	>{c.refColumns[n] || 'Choose a column'}</Select.Trigger
																>
																<Select.Content class="max-h-72">
																	{#each referenceOptions(c) as option (option.name)}<Select.Item
																			value={option.name}
																			label={option.name}
																			>{option.name}{#if option.primaryKey}<KeyRoundIcon
																					class="size-3 text-muted-foreground"
																				/>{/if}</Select.Item
																		>{/each}
																</Select.Content>
															</Select.Root>
														</div>
													{/each}
												</div>
											{/if}
											<div class="flex flex-wrap items-center gap-4">
												{#each FK_ACTIONS as action (action)}
													<div class="flex items-center gap-2">
														<span class="label"
															>{action === 'onUpdate' ? 'On update' : 'On delete'}</span
														>
														<Select.Root
															type="single"
															value={c[action] ?? 'NO ACTION'}
															onValueChange={(v) => {
																if (v === (c[action] ?? 'NO ACTION')) return;
																c[action] = v;
																changed();
															}}
															disabled={locked}
														>
															<Select.Trigger size="sm" class="measured min-w-32 text-xs"
																>{c[action] ?? 'NO ACTION'}</Select.Trigger
															>
															<Select.Content>
																{#each ACTIONS as a (a)}<Select.Item
																		value={a}
																		label={a}
																		class="measured text-xs"
																	/>{/each}
															</Select.Content>
														</Select.Root>
													</div>
												{/each}
											</div>
										{/if}
									</div>
								{/if}
							</section>
						{:else}
							{#if !removedConstraints.length}
								<p class="py-6 text-center text-muted-foreground">
									No constraints. Add a primary key, unique, foreign key or check.
								</p>
							{/if}
						{/each}
						{#each removedConstraints as c (c.name)}
							{@render removedRow(c.name, c.definition, () =>
								restore(() =>
									draft.constraints.push(restored(c, columnRenames(draft, columnNames)))
								)
							)}
						{/each}
						<DropdownMenu.Root>
							<DropdownMenu.Trigger disabled={locked}>
								{#snippet child({ props })}
									<Button {...props} size="sm" variant="outline"
										><PlusIcon /> Add constraint <ChevronDownIcon /></Button
									>
								{/snippet}
							</DropdownMenu.Trigger>
							<DropdownMenu.Content align="start" class="w-48">
								{#each NEW_KINDS as kind (kind)}
									{@const Icon = KINDS[kind].icon}
									<DropdownMenu.Item
										disabled={kind === 'primaryKey' &&
											draft.constraints.some((c) => c.kind === 'primaryKey')}
										onSelect={() => addConstraint(kind)}
									>
										<Icon />{KINDS[kind].label}
									</DropdownMenu.Item>
								{/each}
							</DropdownMenu.Content>
						</DropdownMenu.Root>
					</div>
				{:else}
					<div class="space-y-2 p-3">
						{#each draft.indexes as idx, i (idx)}
							{@const s = indexStatus(idx)}
							<section class="item {s !== 'same' ? `item-${s}` : ''}">
								<div class="flex items-center gap-2">
									<span class="chip shrink-0"><HashIcon class="size-3" />Index</span>
									{#if idx.managed}
										<span class="measured truncate">{idx.name}</span>
										<span class="flex-1"></span>
										<span
											class="chip"
											title="Backs a constraint or uses features this editor does not cover"
											><LockIcon class="size-3" /> Read-only</span
										>
									{:else}
										<input
											class="cell measured max-w-80"
											value={idx.name}
											aria-label="Index name"
											aria-invalid={!idx.name}
											oninput={(e) => {
												idx.name = e.currentTarget.value;
												autoNamed.delete(idx);
											}}
										/>
										<Select.Root
											type="single"
											value={idx.method}
											onValueChange={(v) => {
												idx.method = v;
												changed();
											}}
											disabled={locked}
										>
											<Select.Trigger
												size="sm"
												class="measured min-w-24 text-xs"
												aria-label="Method">{idx.method}</Select.Trigger
											>
											<Select.Content>
												{#each METHODS as m (m)}<Select.Item
														value={m}
														label={m}
														class="measured text-xs"
													/>{/each}
											</Select.Content>
										</Select.Root>
										<label class="flex items-center gap-1.5 text-xs">
											<Checkbox
												checked={idx.unique}
												disabled={locked}
												onCheckedChange={(v) => {
													idx.unique = v;
													changed();
												}}
											/> Unique
										</label>
										<span class="flex-1"></span>
										{@render statusChip(s)}
										<Button
											size="icon-xs"
											variant="ghost"
											disabled={locked}
											title="Remove index"
											aria-label="Remove index {idx.name}"
											onclick={() => {
												draft.indexes.splice(i, 1);
												changed();
											}}><TrashIcon /></Button
										>
									{/if}
								</div>
								{#if idx.managed}
									<p class="measured mt-2 text-xs break-all text-muted-foreground">
										{idx.definition}
									</p>
								{:else}
									<div class="mt-2 flex items-baseline gap-3">
										<span class="label">Columns</span>
										{@render columnChips(idx.columns, (name) => toggleIndexColumn(idx, name), true)}
									</div>
								{/if}
							</section>
						{:else}
							{#if !removedIndexes.length}
								<p class="py-6 text-center text-muted-foreground">No indexes yet.</p>
							{/if}
						{/each}
						{#each removedIndexes as idx (idx.name)}
							{@render removedRow(idx.name, idx.definition, () =>
								restore(() => draft.indexes.push(restored(idx, columnRenames(draft, columnNames))))
							)}
						{/each}
						<Button size="sm" variant="outline" disabled={locked} onclick={addIndex}
							><PlusIcon /> Add index</Button
						>
					</div>
				{/if}
			</fieldset>
		</div>

		<footer class="flex h-9 shrink-0 items-center gap-1 border-t bg-chrome px-2 text-xs">
			{#if readOnlyReason}
				<span class="flex items-center gap-1.5 px-1 text-muted-foreground"
					><LockIcon class="size-3.5" />{readOnlyReason}</span
				>
			{:else if dirty}
				<span class="ml-auto px-1 font-medium text-signal-text">
					<span class="measured">{dirty}</span> pending change{dirty === 1 ? '' : 's'}
				</span>
				<Button size="xs" variant="ghost" disabled={busy} onclick={discard}>Discard</Button>
				<Button size="xs" disabled={busy} onclick={review}>
					{#if busy}<LoaderIcon class="animate-spin" />{/if}
					Review & apply
				</Button>
			{:else}
				<span class="px-1 text-muted-foreground"
					>Nothing runs until you review and apply. Click a key to set the primary key.</span
				>
			{/if}
		</footer>
	{/if}
</div>

<Dialog.Root
	open={reviewing}
	onOpenChange={(open) => {
		if (!open && !busy) reviewing = false;
	}}
>
	<Dialog.Content class="sm:max-w-3xl" showCloseButton={!busy}>
		<Dialog.Header>
			<Dialog.Title>Review changes</Dialog.Title>
			<Dialog.Description>
				{dirty} change{dirty === 1 ? '' : 's'} to
				<span class="measured">{schema}.{draft.name}</span>, run in one transaction.
			</Dialog.Description>
		</Dialog.Header>
		{#if destroys}
			<p
				class="flex items-start gap-2 rounded-lg border border-[color-mix(in_oklab,var(--color-bad),transparent_55%)] bg-bad-soft px-3 py-2 text-xs text-bad"
			>
				<TriangleAlertIcon class="mt-px size-3.5 shrink-0" />
				This drops or alters existing columns, constraints or indexes. Data can be lost or the change
				can fail on existing rows.
			</p>
		{/if}
		{#if error}<p role="alert" class="text-xs text-bad">{error}</p>{/if}
		<pre
			class="measured max-h-[55vh] overflow-auto rounded-lg border bg-background p-3 text-xs whitespace-pre-wrap select-text">{#each sql as statement, n (n)}{#each highlightSql(statement + ';') as token, t (t)}{#if token.classes}<span
							class={token.classes}>{token.text}</span
						>{:else}{token.text}{/if}{/each}{n < sql.length - 1 ? '\n\n' : ''}{/each}</pre>
		<Dialog.Footer>
			<Button variant="outline" disabled={busy} onclick={() => (reviewing = false)}>Cancel</Button>
			<Button variant={destroys ? 'destructive' : 'default'} disabled={busy} onclick={apply}>
				{#if busy}<LoaderIcon class="animate-spin" />{/if}
				Apply {dirty} change{dirty === 1 ? '' : 's'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	.cell {
		height: 1.875rem;
		width: 100%;
		min-width: 0;
		border: 1px solid transparent;
		border-radius: var(--radius-md);
		background: transparent;
		padding-inline: 0.5rem;
		font-size: 0.8125rem;
		transition: border-color var(--dur-micro);
	}
	.cell::placeholder,
	.title-field::placeholder,
	.field::placeholder {
		color: var(--color-muted);
		opacity: 0.7;
	}
	.cell:hover:not(:disabled) {
		border-color: var(--color-rule-2);
	}
	.cell:focus {
		border-color: var(--color-field-edge);
		background: var(--color-paper-3);
	}
	.cell:disabled {
		color: var(--color-muted);
	}
	.cell[aria-invalid='true'],
	.field[aria-invalid='true'],
	.title-field[aria-invalid='true'] {
		border-color: color-mix(in oklab, var(--color-bad), transparent 40%);
	}
	.field {
		height: 2rem;
		width: 100%;
		border: 1px solid var(--color-field-edge);
		border-radius: var(--radius-lg);
		background: var(--color-paper-3);
		padding-inline: 0.625rem;
		font-size: 0.8125rem;
	}
	.title-field {
		height: 1.875rem;
		min-width: 6rem;
		border: 1px solid transparent;
		border-radius: var(--radius-md);
		background: transparent;
		padding-inline: 0.25rem;
		font-size: 0.9375rem;
		font-weight: 500;
		color: var(--color-ink);
	}
	.title-field:hover:not(:disabled) {
		border-color: var(--color-rule-2);
	}
	.title-field:focus {
		border-color: var(--color-field-edge);
		background: var(--color-paper-3);
	}
	.preset-trigger {
		display: grid;
		place-items: center;
		height: 1.875rem;
		width: 1.5rem;
		flex-shrink: 0;
		border-radius: 0 var(--radius-md) var(--radius-md) 0;
		color: var(--color-muted);
	}
	.preset-trigger:hover:not(:disabled),
	.preset-trigger[data-state='open'] {
		background: var(--color-hover);
		color: var(--color-ink);
	}
	.preset-trigger:disabled {
		opacity: 0.4;
	}
	.key-toggle {
		display: grid;
		place-items: center;
		height: 2.375rem;
		width: 100%;
		color: color-mix(in oklab, currentColor, transparent 70%);
		transition: color var(--dur-micro);
	}
	.key-toggle:hover:not(:disabled) {
		color: var(--color-ink);
	}
	.key-toggle[aria-pressed='true'] {
		color: var(--color-accent-text);
	}
	.section-tab {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.625rem;
		border-bottom: 2px solid transparent;
		color: var(--color-muted);
		font-size: 0.8125rem;
		font-weight: 500;
	}
	.section-tab:hover {
		color: var(--color-ink);
	}
	.section-tab[aria-selected='true'] {
		color: var(--color-ink);
		border-color: var(--color-accent);
	}
	.item {
		border: 1px solid var(--color-rule);
		border-left-width: 3px;
		border-radius: var(--radius-lg);
		background: var(--color-paper-3);
		padding: 0.5rem 0.75rem 0.625rem;
	}
	.item-new {
		border-left-color: var(--color-ok);
	}
	.item-changed {
		border-left-color: var(--color-accent);
	}
	.label {
		width: 4.75rem;
		flex-shrink: 0;
		font-size: 0.75rem;
		color: var(--color-muted);
	}
	.chip-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		height: 1.5rem;
		padding-inline: 0.5rem;
		border: 1px solid var(--color-rule-2);
		border-radius: var(--radius-pill);
		font-family: var(--font-outlier);
		font-size: 0.75rem;
		color: var(--color-muted);
		transition:
			background-color var(--dur-micro),
			color var(--dur-micro);
	}
	.chip-toggle:hover:not(:disabled) {
		background: var(--color-hover);
		color: var(--color-ink);
	}
	.chip-toggle[aria-pressed='true'] {
		background: var(--color-accent-soft);
		border-color: color-mix(in oklab, var(--color-accent) 55%, transparent);
		color: var(--color-ink);
	}
	.order {
		display: grid;
		place-items: center;
		min-width: 1rem;
		height: 1rem;
		border-radius: 999px;
		background: var(--color-accent);
		color: var(--color-accent-ink);
		font-size: 0.625rem;
	}
</style>
