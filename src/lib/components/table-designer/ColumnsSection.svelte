<script lang="ts">
	import type { Attachment } from 'svelte/attachments';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import LockIcon from '@lucide/svelte/icons/lock';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import TrashIcon from '@lucide/svelte/icons/trash';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import {
		COLUMN_TYPES,
		IDENTITY_LABELS,
		defaultPresets,
		primaryKeyColumns,
		removeDraftColumn,
		renameDraftColumn,
		restoreColumn,
		supportsIdentity,
		togglePrimaryKey,
		type ChangeStatus,
		type ItemStatus,
		type StructureColumn,
		type TableStructure
	} from '#lib/structure.js';
	import RemovedRow from './RemovedRow.svelte';

	interface Props {
		draft: TableStructure;
		/** Original name of each draft column by position; null for columns added in this draft. */
		columnNames: (string | null)[];
		original: TableStructure | null;
		status: ChangeStatus;
		locked: boolean;
		changed: () => void;
		/** Runs a draft change that may throw, reporting the error instead. */
		attempt: (change: () => void) => void;
		onAdd: () => void;
		focusWhenNew: (item: object) => Attachment<HTMLInputElement>;
	}
	let {
		draft = $bindable(),
		columnNames = $bindable(),
		original,
		status,
		locked,
		changed,
		attempt,
		onAdd,
		focusWhenNew
	}: Props = $props();

	let keyColumns = $derived(primaryKeyColumns(draft));
	let removedColumns = $derived(
		(original?.columns ?? []).filter((c) => status.droppedColumns.includes(c.name))
	);

	/** An existing column whose type differs from the loaded one, so it may need USING. */
	function typeChanged(i: number, column: StructureColumn): boolean {
		const origin = columnNames[i];
		const old = origin ? original?.columns.find((c) => c.name === origin) : undefined;
		return !!old && old.dataType !== column.dataType;
	}

	function columnStatus(i: number): ItemStatus {
		if (!original) return 'same';
		const origin = columnNames[i];
		if (!origin) return 'new';
		return status.alteredColumns.has(origin) ? 'changed' : 'same';
	}
	function setIdentity(column: StructureColumn, value: string) {
		column.identity = value as StructureColumn['identity'];
		if (column.identity) {
			column.nullable = false;
			column.default = null;
		}
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
							const name = e.currentTarget.value.trim();
							attempt(() => renameDraftColumn(draft, column, name));
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
					{#if typeChanged(i, column)}
						<input
							class="cell measured mt-1"
							value={column.using ?? ''}
							placeholder="USING expression"
							title="How existing values convert to the new type, e.g. {column.name}::{column.dataType}"
							aria-label="USING expression"
							disabled={fixed}
							oninput={(e) => (column.using = e.currentTarget.value || null)}
						/>
					{/if}
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
							{IDENTITY_LABELS[column.identity]}
						</Select.Trigger>
						<Select.Content>
							{#each Object.entries(IDENTITY_LABELS) as [value, label] (value)}
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
						onclick={() => {
							removeDraftColumn(draft, columnNames, i);
							changed();
						}}><TrashIcon /></Button
					>
				</td>
			</tr>
		{/each}
	</tbody>
</table>
<datalist id="designer-types">
	{#each COLUMN_TYPES as type (type)}<option value={type}></option>{/each}
</datalist>
<div class="space-y-1.5 p-3">
	{#each removedColumns as column (column.name)}
		<RemovedRow
			label={column.name}
			detail={column.dataType}
			disabled={locked}
			onrestore={() => attempt(() => restoreColumn(original!, draft, columnNames, column.name))}
		/>
	{/each}
	{#if !draft.columns.length && !removedColumns.length}
		<p class="py-6 text-center text-muted-foreground">
			No columns yet. Add the first one to define the table.
		</p>
	{/if}
	<Button size="sm" variant="outline" disabled={locked} onclick={onAdd}
		><PlusIcon /> Add column</Button
	>
</div>

<style>
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
</style>
