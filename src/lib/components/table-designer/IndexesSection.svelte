<script lang="ts">
	import HashIcon from '@lucide/svelte/icons/hash';
	import LockIcon from '@lucide/svelte/icons/lock';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import TrashIcon from '@lucide/svelte/icons/trash';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import {
		INDEX_METHODS,
		columnRenames,
		namedStatus,
		restored,
		toggleIndexColumn,
		type AutoNamed,
		type ChangeStatus,
		type StructureIndex,
		type TableStructure
	} from '#lib/structure.js';
	import ColumnChips from './ColumnChips.svelte';
	import RemovedRow from './RemovedRow.svelte';
	import StatusChip from './StatusChip.svelte';

	interface Props {
		draft: TableStructure;
		columnNames: (string | null)[];
		original: TableStructure | null;
		status: ChangeStatus;
		removed: StructureIndex[];
		columnList: string[];
		autoNamed: AutoNamed & Pick<WeakSet<object>, 'delete'>;
		locked: boolean;
		changed: () => void;
		attempt: (change: () => void) => void;
		onAdd: () => void;
	}
	let {
		draft = $bindable(),
		columnNames,
		original,
		status,
		removed,
		columnList,
		autoNamed,
		locked,
		changed,
		attempt,
		onAdd
	}: Props = $props();
</script>

<div class="space-y-2 p-3">
	{#each draft.indexes as idx, i (idx)}
		{@const s = original ? namedStatus(idx.name, original.indexes, status.addedIndexes) : 'same'}
		<section class="item {s !== 'same' ? `item-${s}` : ''}">
			<div class="flex items-center gap-2">
				<span class="chip shrink-0"><HashIcon class="size-3" />Index</span>
				{#if idx.managed}
					<span class="measured truncate">{idx.name}</span>
					<span class="flex-1"></span>
					<span class="chip" title="Backs a constraint or uses features this editor does not cover"
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
						<Select.Trigger size="sm" class="measured min-w-24 text-xs" aria-label="Method"
							>{idx.method}</Select.Trigger
						>
						<Select.Content>
							{#each INDEX_METHODS as m (m)}<Select.Item
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
					<StatusChip status={s} />
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
					<ColumnChips
						columns={columnList}
						selected={idx.columns}
						disabled={locked}
						ordered
						toggle={(name) => {
							toggleIndexColumn(draft, idx, name, autoNamed);
							changed();
						}}
					/>
				</div>
			{/if}
		</section>
	{:else}
		{#if !removed.length}
			<p class="py-6 text-center text-muted-foreground">No indexes yet.</p>
		{/if}
	{/each}
	{#each removed as idx (idx.name)}
		<RemovedRow
			label={idx.name}
			detail={idx.definition}
			disabled={locked}
			onrestore={() =>
				attempt(() => draft.indexes.push(restored(idx, columnRenames(draft, columnNames))))}
		/>
	{/each}
	<Button size="sm" variant="outline" disabled={locked} onclick={onAdd}
		><PlusIcon /> Add index</Button
	>
</div>
