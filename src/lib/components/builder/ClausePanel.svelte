<script lang="ts" module>
	import type { ColumnRef } from '#lib/query-builder.js';

	export const refKey = (ref: ColumnRef | null) => (ref ? `${ref.tableId}.${ref.column}` : '');
	export function parseRef(key: string): ColumnRef {
		const dot = key.indexOf('.');
		return { tableId: key.slice(0, dot), column: key.slice(dot + 1) };
	}
</script>

<script lang="ts">
	import XIcon from '@lucide/svelte/icons/x';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import {
		AGGREGATES,
		JOIN_TYPES,
		OPERATORS,
		newId,
		takesValue,
		type BuilderModel,
		type JoinType,
		type Operator
	} from '#lib/query-builder.js';
	import Pick from './Pick.svelte';
	import { joinLabel } from './JoinEdge.svelte';

	interface Props {
		model: BuilderModel;
		/** Every column of every table on the canvas. */
		columns: { value: string; label: string }[];
	}

	let { model, columns }: Props = $props();

	const alias = (tableId: string) => model.tables.find((t) => t.id === tableId)?.alias ?? '?';

	const aggregateOptions = [
		{ value: 'none', label: 'value' },
		...AGGREGATES.map((a) => ({ value: a, label: a }))
	];
	const operatorOptions = OPERATORS.map((op) => ({ value: op, label: op }));
	const havingColumns = $derived([
		...columns,
		...(model.tables.length ? [{ value: `${model.tables[0].id}.*`, label: '*' }] : [])
	]);
	const sortOptions = $derived([
		...columns,
		...model.columns
			.filter((c) => c.countAll)
			.map((c) => ({ value: `${c.tableId}.*`, label: 'COUNT(*)' }))
	]);

	function move<T>(list: T[], index: number, by: number) {
		const target = index + by;
		if (target < 0 || target >= list.length) return;
		[list[index], list[target]] = [list[target], list[index]];
	}

	const field =
		'h-7 min-w-0 rounded-md border border-transparent bg-transparent px-2 text-xs placeholder:text-muted-foreground/70 hover:border-rule-2 focus:border-input focus:bg-work focus:outline-none';
	const iconButton =
		'grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-hover hover:text-foreground disabled:opacity-30 disabled:hover:bg-transparent';
	const addButton =
		'flex h-7 items-center gap-1 rounded-md px-2 text-xs text-muted-foreground hover:bg-hover hover:text-foreground disabled:opacity-50';
</script>

{#snippet ref(column: ColumnRef)}<span class="measured inline-block whitespace-nowrap"
		><span class="text-muted-foreground">{alias(column.tableId)}.</span>{column.column}</span
	>{/snippet}

{#snippet heading(title: string, keyword: string)}
	<div class="flex items-baseline gap-2 px-3 pt-3 pb-1">
		<span class="measured text-[11px] font-medium tracking-wide text-signal-text">{keyword}</span>
		<span class="text-xs text-muted-foreground">{title}</span>
	</div>
{/snippet}

<div class="flex flex-col pb-3">
	<div class="flex items-center justify-between pr-3">
		{@render heading('Columns', 'SELECT')}
		<label class="flex items-center gap-1.5 pt-2 text-xs text-ink-2">
			<Checkbox bind:checked={model.distinct} aria-label="Distinct rows" /> distinct
		</label>
	</div>
	{#if model.columns.length === 0}
		<p class="px-3 py-1 text-xs text-muted-foreground">
			All columns (<span class="measured">*</span>). Tick columns on the canvas to pick some.
		</p>
	{/if}
	{#each model.columns as column, i (refKey(column))}
		<div class="group flex flex-wrap items-center gap-x-1 px-2">
			<span class="min-w-36 flex-1 py-1.5 pl-1 text-xs"
				>{#if column.countAll}*{:else}{@render ref(column)}{/if}</span
			>
			<div class="ml-auto flex items-center gap-1">
				<Pick
					label="Aggregate"
					class="w-20"
					value={column.aggregate ?? 'none'}
					options={column.countAll ? [{ value: 'COUNT', label: 'COUNT' }] : aggregateOptions}
					onchange={(v) =>
						(column.aggregate = v === 'none' ? null : (v as typeof column.aggregate))}
				/>
				{#if column.aggregate && !column.countAll}
					<label
						class="flex items-center gap-1 text-xs text-muted-foreground"
						title="Aggregate distinct values"
					>
						<Checkbox
							checked={column.aggregateDistinct ?? false}
							onCheckedChange={(value) => (column.aggregateDistinct = value)}
							aria-label="Distinct aggregate values"
						/> distinct
					</label>
				{/if}
				<input
					class="{field} measured w-24"
					placeholder="alias"
					aria-label="Alias"
					bind:value={column.alias}
				/>
				<button
					type="button"
					class="{iconButton} opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
					aria-label="Move up"
					disabled={i === 0}
					onclick={() => move(model.columns, i, -1)}
				>
					<ArrowUpIcon class="size-3" />
				</button>
				<button
					type="button"
					class={iconButton}
					aria-label="Remove column"
					onclick={() => model.columns.splice(i, 1)}
				>
					<XIcon class="size-3.5" />
				</button>
			</div>
		</div>
	{/each}

	<div class="px-2">
		<button
			type="button"
			class={addButton}
			disabled={!model.tables.length || model.columns.some((column) => column.countAll)}
			onclick={() =>
				model.columns.push({
					tableId: model.tables[0].id,
					column: '*',
					aggregate: 'COUNT',
					alias: '',
					countAll: true,
					builtinAggregate: true
				})}><PlusIcon class="size-3.5" /> Count rows</button
		>
	</div>

	{#if model.tables.length > 1}
		{@render heading('Joins', 'JOIN')}
		{#if model.joins.length === 0}
			<p class="px-3 py-1 text-xs text-muted-foreground">
				Drag a column onto a column of another table to join them.
			</p>
		{/if}
		{#each model.joins as join, i (join.id)}
			<div class="flex flex-wrap items-center gap-x-1 px-2">
				<span class="min-w-44 flex-1 py-1.5 pl-1 text-xs"
					>{@render ref(join.from)} <span class="text-muted-foreground">=</span>
					{@render ref(join.to)}</span
				>
				<div class="ml-auto flex items-center gap-1">
					<Pick
						label="Join type"
						class="w-40"
						value={join.type}
						options={JOIN_TYPES.map((type) => ({
							value: type,
							label: joinLabel(type, alias(join.from.tableId), alias(join.to.tableId))
						}))}
						onchange={(v) => (join.type = v as JoinType)}
					/>
					<button
						type="button"
						class={iconButton}
						aria-label="Remove join"
						onclick={() => model.joins.splice(i, 1)}
					>
						<XIcon class="size-3.5" />
					</button>
				</div>
			</div>
		{/each}
	{/if}

	{@render heading('Conditions', 'WHERE')}
	<p class="px-3 pb-1 text-xs text-muted-foreground">Use parentheses to group conditions.</p>
	{#each model.filters as filter, i (filter.id)}
		<div class="flex flex-wrap items-center gap-x-1 px-2">
			<div class="flex min-w-44 flex-1 items-center gap-1">
				{#if i === 0}
					<span class="measured w-11 shrink-0 pl-1 text-[11px] text-muted-foreground">where</span>
				{:else}
					<button
						type="button"
						class="measured h-6 w-11 shrink-0 rounded-md text-[11px] text-cool hover:bg-hover"
						title="Switch between AND and OR"
						onclick={() => (filter.connector = filter.connector === 'AND' ? 'OR' : 'AND')}
						>{filter.connector.toLowerCase()}</button
					>
				{/if}
				<label
					class="flex items-center gap-0.5 text-xs text-muted-foreground"
					title="Opening parentheses"
				>
					(
					<input
						type="number"
						min="0"
						max="20"
						class="w-9 rounded border border-input bg-background px-1 text-xs"
						aria-label="Opening parentheses"
						value={filter.openGroups ?? 0}
						oninput={(event) => (filter.openGroups = Number(event.currentTarget.value))}
					/>
				</label>
				<Pick
					label="Condition column"
					class="min-w-0 flex-1"
					placeholder="column"
					value={refKey(filter.column)}
					options={columns}
					onchange={(v) => (filter.column = parseRef(v))}
				/>
			</div>
			<div class="flex min-w-44 flex-1 items-center gap-1">
				<Pick
					label="Operator"
					class="w-24"
					value={filter.operator}
					options={operatorOptions}
					onchange={(v) => (filter.operator = v as Operator)}
				/>
				{#if takesValue(filter.operator)}
					<input
						class="{field} measured min-w-0 flex-1"
						placeholder={filter.operator.endsWith('IN') ? 'a, b, c' : 'value'}
						aria-label="Value"
						bind:value={filter.value}
					/>
				{/if}
				<label
					class="flex items-center gap-0.5 text-xs text-muted-foreground"
					title="Closing parentheses"
				>
					<input
						type="number"
						min="0"
						max="20"
						class="w-9 rounded border border-input bg-background px-1 text-xs"
						aria-label="Closing parentheses"
						value={filter.closeGroups ?? 0}
						oninput={(event) => (filter.closeGroups = Number(event.currentTarget.value))}
					/>
					)
				</label>
				<button
					type="button"
					class={iconButton}
					aria-label="Remove condition"
					onclick={() => model.filters.splice(i, 1)}
				>
					<XIcon class="size-3.5" />
				</button>
			</div>
		</div>
	{/each}
	<div class="px-2">
		<button
			type="button"
			class={addButton}
			disabled={columns.length === 0}
			onclick={() =>
				model.filters.push({
					id: newId('f'),
					connector: 'AND',
					column: null,
					operator: '=',
					value: ''
				})}><PlusIcon class="size-3.5" /> Condition</button
		>
	</div>

	{@render heading('Aggregate conditions', 'HAVING')}
	{#each model.having ?? [] as filter, i (filter.id)}
		<div class="flex flex-wrap items-center gap-x-1 px-2">
			<div class="flex min-w-44 flex-1 items-center gap-1">
				{#if i === 0}<span class="measured w-11 shrink-0 pl-1 text-[11px] text-muted-foreground"
						>having</span
					>
				{:else}<button
						type="button"
						class="measured h-6 w-11 shrink-0 text-[11px] text-cool"
						title="Switch between AND and OR"
						onclick={() => (filter.connector = filter.connector === 'AND' ? 'OR' : 'AND')}
						>{filter.connector.toLowerCase()}</button
					>{/if}
				<label
					class="flex items-center gap-0.5 text-xs text-muted-foreground"
					title="Opening parentheses"
					>(
					<input
						type="number"
						min="0"
						max="20"
						class="w-9 rounded border border-input bg-background px-1 text-xs"
						aria-label="Opening HAVING parentheses"
						value={filter.openGroups ?? 0}
						oninput={(event) => (filter.openGroups = Number(event.currentTarget.value))}
					/>
				</label>
				<Pick
					label="HAVING aggregate"
					class="w-20"
					value={filter.havingAggregate ?? 'COUNT'}
					options={(filter.havingCountAll ? ['COUNT'] : AGGREGATES).map((a) => ({
						value: a,
						label: a
					}))}
					onchange={(v) => (filter.havingAggregate = v as typeof filter.havingAggregate)}
				/>
				<Pick
					label="HAVING column"
					class="min-w-0 flex-1"
					placeholder="column"
					value={refKey(filter.column)}
					options={havingColumns}
					onchange={(v) => {
						filter.column = parseRef(v);
						filter.havingCountAll = filter.column.column === '*';
						if (filter.havingCountAll) {
							filter.havingAggregate = 'COUNT';
							filter.havingDistinct = false;
						}
					}}
				/>
			</div>
			<div class="flex min-w-44 flex-1 items-center gap-1">
				{#if !filter.havingCountAll}<label
						class="flex items-center gap-1 text-xs text-muted-foreground"
						><Checkbox
							checked={filter.havingDistinct ?? false}
							onCheckedChange={(value) => (filter.havingDistinct = value)}
							aria-label="Distinct HAVING values"
						/> distinct</label
					>{/if}
				<Pick
					label="HAVING operator"
					class="w-24"
					value={filter.operator}
					options={operatorOptions}
					onchange={(v) => (filter.operator = v as Operator)}
				/>
				{#if takesValue(filter.operator)}<input
						class="{field} measured min-w-0 flex-1"
						placeholder="value"
						aria-label="HAVING value"
						bind:value={filter.value}
					/>{/if}
				<label
					class="flex items-center gap-0.5 text-xs text-muted-foreground"
					title="Closing parentheses"
					><input
						type="number"
						min="0"
						max="20"
						class="w-9 rounded border border-input bg-background px-1 text-xs"
						aria-label="Closing HAVING parentheses"
						value={filter.closeGroups ?? 0}
						oninput={(event) => (filter.closeGroups = Number(event.currentTarget.value))}
					/> )</label
				>
				<button
					type="button"
					class={iconButton}
					aria-label="Remove HAVING condition"
					onclick={() => model.having?.splice(i, 1)}><XIcon class="size-3.5" /></button
				>
			</div>
		</div>
	{/each}
	<div class="px-2">
		<button
			type="button"
			class={addButton}
			disabled={!model.tables.length}
			onclick={() => {
				model.having ??= [];
				model.having.push({
					id: newId('h'),
					connector: 'AND',
					column: null,
					operator: '>',
					value: '',
					havingAggregate: 'COUNT',
					havingBuiltin: true
				});
			}}><PlusIcon class="size-3.5" /> Aggregate condition</button
		>
	</div>

	{@render heading('Sort', 'ORDER BY')}
	{#each model.sorts as sort, i (sort.id)}
		<div class="flex items-center gap-1 px-2">
			<Pick
				label="Sort column"
				class="min-w-0 flex-1"
				placeholder="column"
				value={refKey(sort.column)}
				options={sortOptions}
				onchange={(v) => (sort.column = parseRef(v))}
			/>
			<button
				type="button"
				class="measured flex h-7 w-20 shrink-0 items-center justify-center gap-1 rounded-md text-xs text-ink-2 hover:bg-hover"
				title="Switch direction"
				onclick={() => (sort.direction = sort.direction === 'ASC' ? 'DESC' : 'ASC')}
			>
				{#if sort.direction === 'ASC'}<ArrowUpIcon class="size-3" /> asc{:else}<ArrowDownIcon
						class="size-3"
					/> desc{/if}
			</button>
			<button
				type="button"
				class={iconButton}
				aria-label="Remove sort"
				onclick={() => model.sorts.splice(i, 1)}
			>
				<XIcon class="size-3.5" />
			</button>
		</div>
	{/each}
	<div class="px-2">
		<button
			type="button"
			class={addButton}
			disabled={columns.length === 0}
			onclick={() => model.sorts.push({ id: newId('s'), column: null, direction: 'ASC' })}
			><PlusIcon class="size-3.5" /> Sort</button
		>
	</div>

	{@render heading('Rows', 'LIMIT')}
	<div class="flex items-center gap-2 px-3 text-xs text-muted-foreground">
		<label class="flex items-center gap-1.5"
			>limit
			<input
				class="{field} measured border-rule w-20"
				inputmode="numeric"
				placeholder="none"
				bind:value={model.limit}
			/></label
		>
		<label class="flex items-center gap-1.5"
			>offset
			<input
				class="{field} measured border-rule w-20"
				inputmode="numeric"
				placeholder="0"
				bind:value={model.offset}
			/></label
		>
	</div>
</div>
