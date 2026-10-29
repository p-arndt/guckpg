<script lang="ts">
	import { onMount } from 'svelte';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import { editView } from '#lib/view-actions.js';
	import { catalog } from '#lib/state/catalog.js';
	import type { RelationKind } from '#lib/api.js';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import XIcon from '@lucide/svelte/icons/x';
	import ListFilterIcon from '@lucide/svelte/icons/list-filter';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { errorMessage } from '#lib/api.js';
	import { formatCount, formatDuration } from '#lib/format.js';
	import {
		FILTER_OPERATORS,
		isComplete,
		takesValue,
		type FilterCondition
	} from '#lib/table-filter.js';
	import { tabs, type TableTab } from '#lib/state/tabs.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { confirmDiscard } from '#lib/state/confirm.svelte.js';
	import ResultGrid from './ResultGrid.svelte';
	import TableDesigner from './TableDesigner.svelte';
	import CsvTransfer from './CsvTransfer.svelte';
	import TestDataDialog from './TestDataDialog.svelte';

	interface Props {
		tab: TableTab;
	}

	let { tab }: Props = $props();

	const result = $derived(tab.output?.results[0]);
	const connection = $derived(connections.get(tab.connectionId));
	const columnTypes = $derived(Object.fromEntries(tab.columns.map((c) => [c.name, c.dataType])));
	const primaryKeys = $derived(tab.columns.filter((c) => c.primaryKey).map((c) => c.name));
	// The result carries column names too, so the filter works before the column list arrives.
	const filterColumns = $derived(
		tab.columns.length ? tab.columns.map((c) => c.name) : (result?.columns ?? [])
	);
	function pageValues(column: string): string[] {
		const index = result?.columns.indexOf(column) ?? -1;
		if (index < 0 || !result) return [];
		return [
			...new Set(
				result.rows
					.map((row) => row[index])
					.filter((value): value is string => value !== null && value !== '')
			)
		].slice(0, 100);
	}
	let relationKind = $state<RelationKind | null>(null);
	$effect(() => {
		const { connectionId, database, schema, table } = tab;
		let current = true;
		relationKind = null;
		catalog.relations(connectionId, database, schema).then(
			(relations) => {
				if (current) relationKind = relations.find((r) => r.name === table)?.kind ?? null;
			},
			() => {
				if (current) relationKind = null;
			}
		);
		return () => {
			current = false;
		};
	});

	type Draft = FilterCondition & { id: string };
	const blankDraft = (): Draft => ({
		id: crypto.randomUUID(),
		column: '',
		op: 'contains',
		value: '',
		connector: 'AND'
	});
	// svelte-ignore state_referenced_locally
	let drafts = $state<Draft[]>(
		tab.filters.length
			? tab.filters.map((f) => ({ ...f, id: crypto.randomUUID() }))
			: [blankDraft()]
	);
	let filterOpen = $state(false);
	let filterError = $state(false);
	let testDataOpen = $state(false);
	let designerOpened = $state(false);
	$effect(() => {
		if (tab.view === 'structure') designerOpened = true;
	});

	onMount(() => void tab.open());

	/** Run an action that reloads the rows, unless the user keeps unsaved edits. */
	async function guarded(action: () => void) {
		if (await confirmDiscard(tab.dirty)) action();
	}

	function toggleFilter() {
		if (!filterOpen) {
			drafts = tab.filters.length
				? tab.filters.map((filter) => ({ ...filter, id: crypto.randomUUID() }))
				: [blankDraft()];
			filterError = false;
		}
		filterOpen = !filterOpen;
	}

	function applyFilter() {
		if (drafts.some((draft) => !isComplete(draft))) {
			filterError = true;
			return;
		}
		filterError = false;
		const filters = drafts.map(({ column, op, value, connector }) => ({
			column,
			op,
			value,
			connector
		}));
		void guarded(() => {
			void tab.applyFilter(filters);
			filterOpen = false;
		});
	}

	function clearFilter() {
		void guarded(() => {
			drafts = [blankDraft()];
			filterError = false;
			void tab.applyFilter([]);
		});
	}

	const load = () => tab.load();

	function structureApplied(name: string) {
		drafts = [blankDraft()];
		void tab.structureApplied(name);
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div
		class="flex h-10 shrink-0 items-center gap-1 border-b px-3"
		role="tablist"
		aria-label="Table view"
	>
		{#each ['data', 'structure'] as view (view)}
			<Button
				role="tab"
				aria-selected={tab.view === view}
				variant={tab.view === view ? 'secondary' : 'ghost'}
				size="sm"
				disabled={tab.structureBusy || tab.running}
				onclick={() => tab.switchView(view as 'data' | 'structure')}
				>{view === 'data' ? 'Data' : 'Structure'}{view === 'structure' && tab.structureDirty
					? ` (${tab.structureDirty})`
					: ''}</Button
			>
		{/each}
		{#if relationKind === 'view'}
			<Button
				size="sm"
				variant="outline"
				class="ml-auto"
				disabled={connection?.readOnly ?? true}
				onclick={() => editView(tab.connectionId, tab.database, tab.schema, tab.table)}
				><PencilIcon /> Edit view</Button
			>
		{/if}
	</div>
	<div
		class="relative min-h-0 flex-1 flex-col"
		class:flex={tab.view === 'data'}
		class:hidden={tab.view !== 'data'}
	>
		<div class="flex h-11 shrink-0 items-center gap-1 border-b px-2">
			<Button
				size="icon-sm"
				variant="ghost"
				disabled={tab.running}
				onclick={() => guarded(load)}
				title="Refresh"
				aria-label="Refresh"
			>
				<RefreshCwIcon class={tab.running ? 'animate-spin' : ''} />
			</Button>
			<Button
				size="icon-sm"
				variant="ghost"
				disabled={tab.running || tab.offset === 0}
				onclick={() => guarded(() => tab.page(-1))}
				title="Previous page"
				aria-label="Previous page"
			>
				<ChevronLeftIcon />
			</Button>
			<span class="min-w-28 text-center text-xs text-muted-foreground">
				{#if result?.rows.length}
					<span class="measured text-ink-2">{formatCount(tab.offset + 1)}</span>
					to
					<span class="measured text-ink-2">{formatCount(tab.offset + result.rows.length)}</span>
				{:else}
					no rows
				{/if}
			</span>
			<Button
				size="icon-sm"
				variant="ghost"
				disabled={tab.running || (result?.rows.length ?? 0) < tab.limit}
				onclick={() => guarded(() => tab.page(1))}
				title="Next page"
				aria-label="Next page"
			>
				<ChevronRightIcon />
			</Button>
			<Select.Root
				type="single"
				value={String(tab.limit)}
				onValueChange={(v) => guarded(() => tab.setLimit(Number(v)))}
			>
				<Select.Trigger size="sm" class="ml-1 h-7 text-[13px]" aria-label="Rows per page"
					>{tab.limit} / page</Select.Trigger
				>
				<Select.Content>
					{#each [100, 200, 500, 1000, 5000] as n (n)}
						<Select.Item value={String(n)} label={String(n)} />
					{/each}
				</Select.Content>
			</Select.Root>

			<Button
				size="sm"
				variant={filterOpen || tab.filters.length ? 'secondary' : 'ghost'}
				aria-expanded={filterOpen}
				aria-controls="table-filters-{tab.id}"
				onclick={toggleFilter}
			>
				<ListFilterIcon /> Filter{tab.filters.length ? ` (${tab.filters.length})` : ''}
			</Button>

			<Button
				size="sm"
				variant="outline"
				disabled={tab.running || tab.dirty > 0 || (connection?.readOnly ?? true)}
				onclick={() => (testDataOpen = true)}>Generate test data</Button
			>
			<CsvTransfer
				connectionId={tab.connectionId}
				database={tab.database}
				schema={tab.schema}
				table={tab.table}
				readOnly={connection?.readOnly ?? true}
				disabled={tab.running || tab.dirty > 0}
				onImported={load}
			/>
			<code class="ml-3 min-w-0 flex-1 truncate text-xs text-muted-foreground select-text">
				{tab.lastSql}
			</code>
			<Button
				size="sm"
				variant="outline"
				onclick={() => tabs.openQuery(tab.connectionId, tab.database, tab.lastSql + ';')}
			>
				<SquareTerminalIcon /> Open as query
			</Button>
		</div>

		{#if filterOpen}
			<form
				id="table-filters-{tab.id}"
				aria-label="Filter rows"
				class="shrink-0 border-b bg-muted/30 px-3 py-2"
				onsubmit={(event) => {
					event.preventDefault();
					applyFilter();
				}}
			>
				<div class="flex max-h-[min(9rem,35vh)] max-w-2xl flex-col gap-1.5 overflow-auto">
					{#each drafts as draft, i (draft.id)}
						<div
							class="grid grid-cols-[4rem_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_2rem] items-center gap-1.5"
						>
							{#if i > 0}
								<label>
									<span class="sr-only">Match</span>
									<select
										class="h-8 w-full rounded-md border bg-background px-2 text-sm text-foreground"
										aria-label="Condition connector"
										bind:value={draft.connector}
										disabled={tab.running}
									>
										<option value="AND">AND</option>
										<option value="OR">OR</option>
									</select>
								</label>
							{:else}
								<span class="pl-2 text-xs text-muted-foreground">Where</span>
							{/if}
							<label class="min-w-0">
								<span class="sr-only">Column</span>
								<select
									class="h-8 w-full rounded-md border bg-background px-2 text-sm text-foreground"
									aria-label="Filter column"
									bind:value={draft.column}
									disabled={tab.running}
								>
									<option value="">Choose column</option>
									{#each filterColumns as name (name)}<option value={name}>{name}</option>{/each}
								</select>
							</label>
							<label class="min-w-0">
								<span class="sr-only">Operator</span>
								<select
									class="h-8 w-full rounded-md border bg-background px-2 text-sm text-foreground"
									aria-label="Filter operator"
									bind:value={draft.op}
									disabled={tab.running}
								>
									{#each FILTER_OPERATORS as operator (operator.value)}<option
											value={operator.value}>{operator.label}</option
										>{/each}
								</select>
							</label>
							{#if takesValue(draft.op)}
								<label class="min-w-0">
									<span class="sr-only">Value</span>
									<Input
										class="h-8"
										aria-label="Filter value"
										placeholder="Value (suggestions from this page)"
										list="filter-values-{draft.id}"
										bind:value={draft.value}
										disabled={tab.running}
									/>
									<datalist id="filter-values-{draft.id}">
										{#each pageValues(draft.column) as value (value)}<option {value}
											></option>{/each}
									</datalist>
								</label>
							{:else}
								<span></span>
							{/if}
							<Button
								size="icon-sm"
								variant="ghost"
								type="button"
								title="Remove condition"
								aria-label="Remove condition"
								disabled={tab.running}
								onclick={() => {
									drafts.splice(i, 1);
									if (!drafts.length) drafts.push(blankDraft());
								}}
							>
								<XIcon />
							</Button>
						</div>
					{/each}
				</div>
				{#if drafts.slice(1).some((draft) => draft.connector === 'AND') && drafts
						.slice(1)
						.some((draft) => draft.connector === 'OR')}
					<p class="mt-2 text-xs text-muted-foreground">
						AND conditions are evaluated before OR conditions.
					</p>
				{/if}
				{#if filterError}<p class="mt-2 text-xs text-bad" role="alert">
						Choose a column and enter a value for each condition.
					</p>{/if}
				<div class="mt-1.5 flex items-center gap-2">
					<Button
						size="sm"
						variant="ghost"
						type="button"
						disabled={tab.running}
						onclick={() => drafts.push(blankDraft())}><PlusIcon /> Add condition</Button
					>
					<Button class="ml-auto" size="sm" type="submit" disabled={tab.running}
						>Apply filters</Button
					>
					{#if tab.filters.length}
						<Button
							size="sm"
							variant="ghost"
							type="button"
							disabled={tab.running}
							onclick={clearFilter}>Clear filters</Button
						>
					{/if}
				</div>
			</form>
		{:else if tab.filters.length}
			<div
				class="flex min-h-10 shrink-0 items-center gap-2 border-b border-cool/30 bg-cool/10 px-3 text-sm"
				aria-label="Active filters"
			>
				<ListFilterIcon class="size-4 shrink-0 text-cool" />
				<span class="shrink-0 font-semibold text-cool">Filtered</span>
				<div class="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto whitespace-nowrap">
					{#each tab.filters as filter, i (i)}
						{#if i > 0}<span class="text-xs text-muted-foreground">{filter.connector}</span>{/if}
						<span class="rounded border bg-background px-2 py-0.5 text-xs">
							<strong>{filter.column}</strong>
							{FILTER_OPERATORS.find((operator) => operator.value === filter.op)?.label}
							{#if takesValue(filter.op)}<span class="text-cool">{filter.value}</span>{/if}
						</span>
					{/each}
				</div>
				<Button size="sm" variant="ghost" onclick={toggleFilter}>Edit</Button>
				<Button size="sm" variant="ghost" disabled={tab.running} onclick={clearFilter}
					>Clear filters</Button
				>
			</div>
		{/if}
		<div class="min-h-0 flex-1">
			{#if tab.error}
				<div
					class="m-3 flex max-w-3xl items-start gap-2.5 rounded-2xl border border-[color-mix(in_oklab,var(--color-bad),transparent_55%)] bg-bad-soft p-4 select-text"
					role="alert"
				>
					<CircleAlertIcon class="mt-0.5 size-4 shrink-0 text-bad" />
					<p class="min-w-0 font-medium break-words whitespace-pre-wrap">
						{errorMessage(tab.error)}
					</p>
				</div>
			{:else if result}
				{#key tab.gridVersion}
					<ResultGrid
						columns={result.columns}
						rows={result.rows}
						rowOffset={tab.offset}
						sort={tab.sort}
						onSort={(column) => guarded(() => tab.sortBy(column))}
						{primaryKeys}
						{columnTypes}
						source={result.source}
						editTarget={connection
							? {
									connectionId: tab.connectionId,
									database: tab.database,
									readOnly: connection.readOnly
								}
							: null}
						onSaved={load}
						onDirtyChange={(n) => tab.setGridDirty(n)}
					/>
				{/key}
			{:else}
				<div
					class="h-full overflow-hidden bg-background"
					aria-busy="true"
					aria-label="Loading rows"
				>
					<div class="h-[32px] border-b"></div>
					{#each { length: 12 }, i (i)}
						<div class="border-rule/50 flex h-[28px] items-center gap-6 border-b px-3">
							<span class="h-2 w-6 rounded-sm bg-hover"></span>
							<span class="h-2 rounded-sm bg-hover" style:width="{90 + ((i * 37) % 70)}px"></span>
							<span class="h-2 rounded-sm bg-hover" style:width="{140 + ((i * 53) % 90)}px"></span>
							<span class="h-2 w-20 rounded-sm bg-hover"></span>
						</div>
					{/each}
				</div>
			{/if}
		</div>

		<div
			class="flex h-8 shrink-0 items-center gap-4 border-t px-3 text-xs text-muted-foreground"
			role="status"
		>
			{#if tab.running}
				<LoaderIcon class="size-3.5 animate-spin text-cool" />
			{/if}
			{#if tab.output}
				<span
					><span class="measured text-cool">{formatCount(result?.rows.length ?? 0)}</span> rows</span
				>
				<span class="measured text-cool">{formatDuration(tab.output.elapsedMs)}</span>
			{/if}
			<span class="measured ml-auto truncate"
				>{tab.database} / <span class="text-ink-2">{tab.schema}.{tab.table}</span></span
			>
		</div>
	</div>
	{#if designerOpened}
		<div class="min-h-0 flex-1" class:hidden={tab.view !== 'structure'}>
			<TableDesigner
				connectionId={tab.connectionId}
				database={tab.database}
				schema={tab.schema}
				table={tab.table}
				onApplied={structureApplied}
				onDirtyChange={(count) => tab.setStructureDirty(count)}
				onBusyChange={(busy) => (tab.structureBusy = busy)}
			/>
		</div>
	{/if}
</div>

<TestDataDialog
	bind:open={testDataOpen}
	connectionId={tab.connectionId}
	database={tab.database}
	schema={tab.schema}
	table={tab.table}
	readOnly={connection?.readOnly ?? true}
	onInserted={load}
/>
