<script lang="ts">
	import { Handle, Position, type NodeProps, type Node } from '@xyflow/svelte';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import LinkIcon from '@lucide/svelte/icons/link';
	import TableIcon from '@lucide/svelte/icons/table-2';
	import EyeIcon from '@lucide/svelte/icons/eye';
	import {
		BODY_PAD_PX,
		HEADER_PX,
		ROW_PX,
		TABLE_HANDLE,
		sourceHandle,
		targetHandle,
		type Side,
		type TableNodeData
	} from '#lib/er-layout.js';

	let { data, width, selected }: NodeProps<Node<TableNodeData, 'table'>> = $props();

	const fk = $derived(new Set(data.foreignKeyColumns));
	const sides: Side[] = ['left', 'right'];
	const handleClass = '!size-1.5 !min-h-0 !min-w-0 !border-0 !bg-transparent';
</script>

{#snippet handles(column: string)}
	{#each sides as side (side)}
		<Handle
			type="target"
			position={side === 'left' ? Position.Left : Position.Right}
			id={targetHandle(column, side)}
			isConnectable={false}
			class={handleClass}
		/>
		<Handle
			type="source"
			position={side === 'left' ? Position.Left : Position.Right}
			id={sourceHandle(column, side)}
			isConnectable={false}
			class={handleClass}
		/>
	{/each}
{/snippet}

<div
	class="overflow-hidden rounded-lg border bg-card text-card-foreground shadow-sm transition-shadow {selected
		? 'shadow-md ring-2 ring-primary/60'
		: ''}"
	style:width="{width}px"
	style:border-left-color={data.hue === null ? undefined : `oklch(0.68 0.14 ${data.hue})`}
	style:border-left-width={data.hue === null ? undefined : '3px'}
>
	<div
		class="relative flex items-center gap-1.5 bg-muted/70 px-2.5 text-[13px] font-semibold {data.compact
			? ''
			: 'border-b'}"
		style:height="{HEADER_PX}px"
		style:background-color={data.hue === null ? undefined : `oklch(0.65 0.12 ${data.hue} / 0.2)`}
		title={data.group ? `${data.group}_*` : undefined}
	>
		{@render handles(TABLE_HANDLE)}
		{#if data.kind === 'view' || data.kind === 'materialized'}
			<EyeIcon class="size-3.5 shrink-0 text-sky-500" />
		{:else}
			<TableIcon class="size-3.5 shrink-0 text-amber-600 dark:text-amber-400" />
		{/if}
		<span class="truncate">{data.name}</span>
		{#if data.kind !== 'table'}
			<span class="ml-auto text-[10px] font-normal text-muted-foreground uppercase"
				>{data.kind}</span
			>
		{/if}
	</div>
	{#if !data.compact}
		<div class="font-mono text-[11.5px]" style:padding-block="{BODY_PAD_PX}px">
			{#each data.columns as column (column.name)}
				<div class="relative flex items-center gap-1.5 px-2.5" style:height="{ROW_PX}px">
					{@render handles(column.name)}
					<span class="flex w-3.5 shrink-0 justify-center">
						{#if column.primaryKey}
							<KeyRoundIcon class="size-3 text-amber-500" />
						{:else if fk.has(column.name)}
							<LinkIcon class="size-3 text-sky-500" />
						{/if}
					</span>
					<span class="truncate {column.primaryKey ? 'font-semibold' : ''}">{column.name}</span>
					<span class="ml-auto shrink-0 truncate pl-2 text-muted-foreground">
						{column.dataType}{column.notNull ? '' : '?'}
					</span>
				</div>
			{/each}
			{#if data.hiddenColumns > 0}
				<div
					class="flex items-center px-2.5 pl-8 text-muted-foreground italic"
					style:height="{ROW_PX}px"
				>
					+ {data.hiddenColumns} more {data.hiddenColumns === 1 ? 'column' : 'columns'}
				</div>
			{:else if data.columns.length === 0}
				<div class="px-2.5 text-muted-foreground italic" style:height="{ROW_PX}px">no columns</div>
			{/if}
		</div>
	{/if}
</div>
