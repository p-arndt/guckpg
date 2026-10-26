<script lang="ts">
	import { plural } from '#lib/format.js';
	import { onMount, untrack } from 'svelte';
	import type { Attachment } from 'svelte/attachments';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import LockIcon from '@lucide/svelte/icons/lock';
	import RefreshIcon from '@lucide/svelte/icons/refresh-cw';
	import Table2Icon from '@lucide/svelte/icons/table-2';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
	import { api, errorMessage, type Relation, type Column } from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { catalog } from '#lib/state/catalog.js';
	import { confirmation } from '#lib/state/confirm.svelte.js';
	import { joinStatements, SqlReview } from '#lib/review.svelte.js';
	import {
		DESIGNER_SECTIONS,
		addDraftColumn,
		addDraftConstraint,
		addDraftIndex,
		buildRequest,
		changeStatus,
		designerReadOnlyReason,
		destructive,
		draftChangeCount,
		matchReferences,
		referenceKey,
		referenceOptions as referenceOptionsOf,
		referenceLoads,
		referencesDraft,
		removedItems,
		renameDraftTable,
		structureApi,
		type DesignerSection,
		type NewConstraintKind,
		type StructureConstraint,
		type TableStructure
	} from '#lib/structure.js';
	import ReviewSqlDialog from './ReviewSqlDialog.svelte';
	import ColumnsSection from './table-designer/ColumnsSection.svelte';
	import ConstraintsSection from './table-designer/ConstraintsSection.svelte';
	import IndexesSection from './table-designer/IndexesSection.svelte';

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
		reloading = $state(false),
		error = $state(''),
		previewKey = $state(''),
		schemas = $state<string[]>([]),
		relations = $state<Record<string, Relation[]>>({}),
		refColumns = $state<Record<string, Column[]>>({});
	const review = new SqlReview();
	let section = $state<DesignerSection>('columns');
	/** Constraints and indexes whose name still follows their columns until the user types one. */
	const autoNamed = new WeakSet<object>();
	let focusNext: object | null = null;

	let request = $derived(buildRequest(schema, original, draft, columnNames));
	let requestKey = $derived(JSON.stringify(request));
	let status = $derived(changeStatus(request));
	let dirty = $derived(draftChangeCount(draft, request, table, reloadRequired));
	let readOnlyReason = $derived(
		designerReadOnlyReason({
			reloadRequired,
			readOnly: connections.get(connectionId)?.readOnly,
			table,
			original
		})
	);
	let busy = $derived(review.busy || reloading);
	/** Load, reload and apply failures share one banner with the review flow's own errors. */
	let shownError = $derived(review.error || error);
	let locked = $derived(!!readOnlyReason || busy);
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
			const needed = referenceLoads(draft, schema);
			await Promise.all([
				catalog.schemas(connectionId, database).then((names) => (schemas = names)),
				// Fresh: a table applied a moment ago has to be offered as a reference target.
				...[schema, ...needed.schemas].map((name) =>
					catalog
						.relations(connectionId, database, name, true)
						.then((list) => (relations[name] = list))
				),
				...needed.foreignKeys.map((c) => references(c))
			]);
		} catch (e) {
			error = errorMessage(e);
		} finally {
			loading = false;
		}
	});

	function changed() {
		review.sql = [];
		review.error = '';
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

	function attempt(change: () => void) {
		changed();
		try {
			change();
		} catch (e) {
			error = errorMessage(e);
		}
	}

	function addColumn() {
		focusNext = addDraftColumn(draft, columnNames);
		section = 'columns';
		changed();
	}
	function addConstraint(kind: NewConstraintKind) {
		autoNamed.add(addDraftConstraint(draft, schema, kind));
		section = 'constraints';
		changed();
	}
	function addIndex() {
		autoNamed.add(addDraftIndex(draft));
		section = 'indexes';
		changed();
	}

	const referenceOptions = (c: StructureConstraint) =>
		referenceOptionsOf(draft, original, schema, refColumns, c);
	async function references(c: StructureConstraint) {
		if (!c.refTable || referencesDraft(draft, original, schema, c)) return;
		const key = referenceKey(schema, c);
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
				relations[value] = await catalog.relations(connectionId, database, value);
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

	async function preview() {
		if (busy || !dirty) return;
		error = '';
		const key = requestKey;
		const ok = await review.preview(() => structureApi.preview(request));
		previewKey = ok ? key : '';
	}
	async function apply() {
		if (busy || !previewKey || previewKey !== requestKey) return;
		error = '';
		await review.apply(async () => {
			const current = JSON.parse(requestKey) as typeof request;
			const name = await structureApi.apply(connectionId, database, current);
			review.close(true);
			try {
				await load(name);
			} catch (e) {
				reloadRequired = true;
				original = null;
				draft = { ...empty(), name };
				columnNames = [];
				error = `Changes applied, but reloading failed: ${errorMessage(e)}`;
			}
			review.sql = [];
			previewKey = '';
			onDirtyChange?.(0);
			onApplied?.(name);
		});
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
		reloading = true;
		try {
			await load(reloadRequired ? draft.name : (original?.name ?? table ?? draft.name));
			reloadRequired = false;
			changed();
		} catch (e) {
			error = errorMessage(e);
		} finally {
			reloading = false;
		}
	}
	function discard() {
		draft = original ? structuredClone($state.snapshot(original)) : empty();
		columnNames = original ? draft.columns.map((c) => c.name) : [];
		changed();
	}
</script>

<div class="designer flex h-full min-h-0 flex-col bg-background text-sm">
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
							renameDraftTable(draft, schema, e.currentTarget.value, autoNamed);
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
				{#each DESIGNER_SECTIONS as item (item)}
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

		{#if shownError}
			<p
				role="alert"
				class="flex shrink-0 items-start gap-2 border-b bg-bad-soft px-3 py-2 text-xs text-bad"
			>
				<TriangleAlertIcon class="mt-px size-3.5 shrink-0" />{shownError}
			</p>
		{/if}

		<div class="min-h-0 flex-1 overflow-auto">
			<fieldset disabled={locked} class="min-w-0" onchange={changed} oninput={changed}>
				{#if section === 'columns'}
					<ColumnsSection
						bind:draft
						bind:columnNames
						{original}
						{status}
						{locked}
						{changed}
						{attempt}
						{focusWhenNew}
						onAdd={addColumn}
					/>
				{:else if section === 'constraints'}
					<ConstraintsSection
						bind:draft
						{columnNames}
						{original}
						{status}
						removed={removedConstraints}
						{schema}
						{schemas}
						{relations}
						{columnList}
						{autoNamed}
						{locked}
						{changed}
						{attempt}
						{referenceOptions}
						onAdd={addConstraint}
						onRefSchema={setRefSchema}
						onRefTable={setRefTable}
					/>
				{:else}
					<IndexesSection
						bind:draft
						{columnNames}
						{original}
						{status}
						removed={removedIndexes}
						{columnList}
						{autoNamed}
						{locked}
						{changed}
						{attempt}
						onAdd={addIndex}
					/>
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
				<Button size="xs" disabled={busy} onclick={preview}>
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

<ReviewSqlDialog
	open={review.reviewing}
	{busy}
	title="Review changes"
	sql={joinStatements(review.sql, '\n\n')}
	error={shownError}
	destructive={destroys}
	confirmLabel="Apply {plural(dirty, 'change')}"
	onConfirm={apply}
	onClose={() => review.close()}
>
	{#snippet description()}
		{plural(dirty, 'change')} to
		<span class="measured">{schema}.{draft.name}</span>, run in one transaction.
	{/snippet}
	{#if destroys}
		<p
			class="flex items-start gap-2 rounded-lg border border-[color-mix(in_oklab,var(--color-bad),transparent_55%)] bg-bad-soft px-3 py-2 text-xs text-bad"
		>
			<TriangleAlertIcon class="mt-px size-3.5 shrink-0" />
			This drops or alters existing columns, constraints or indexes. Data can be lost or the change can
			fail on existing rows.
		</p>
	{/if}
</ReviewSqlDialog>

<style>
	/* The section components share these field styles, so they apply across the whole designer. */
	.designer :global {
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
</style>
