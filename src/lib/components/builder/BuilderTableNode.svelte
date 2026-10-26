<script lang="ts" module>
	export interface BuilderNodeColumn {
		name: string;
		dataType: string;
		primaryKey: boolean;
	}

	export interface BuilderNodeData extends Record<string, unknown> {
		alias: string;
		name: string;
		schema: string;
		columns: BuilderNodeColumn[];
		selected: string[];
		loading: boolean;
		ontoggle: (column: string) => void;
		ontoggleall: (on: boolean) => void;
		onremove: () => void;
	}

	export const leftHandle = (column: string) => `l:${column}`;
	export const rightHandle = (column: string) => `r:${column}`;
	export const columnOfHandle = (handle: string | null | undefined) => handle?.slice(2) ?? null;
</script>

<script lang="ts">
	import { Handle, Position, type Node, type NodeProps } from '@xyflow/svelte';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import XIcon from '@lucide/svelte/icons/x';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { kindStyle } from '../tree/kinds.js';

	let { data, selected }: NodeProps<Node<BuilderNodeData, 'builderTable'>> = $props();

	const chosen = $derived(new Set(data.selected));
	const all = $derived(data.columns.length > 0 && chosen.size === data.columns.length);
	const table = kindStyle('table');
</script>

<div
	class="w-60 overflow-hidden rounded-xl border bg-raised text-foreground shadow-panel {selected
		? 'border-signal'
		: 'border-rule-2'}"
>
	<div class="flex h-8 items-center gap-1.5 border-b bg-chrome pr-1 pl-2.5 text-[13px] font-medium">
		<Checkbox
			class="nodrag"
			checked={all}
			indeterminate={chosen.size > 0 && !all}
			aria-label="Select all columns of {data.alias}"
			onCheckedChange={(on) => data.ontoggleall(on)}
		/>
		<table.icon class="size-3.5 shrink-0 {table.color}" />
		<span class="truncate" title="{data.schema}.{data.name}">{data.name}</span>
		{#if data.alias !== data.name}
			<span class="truncate text-xs font-normal text-muted-foreground">as {data.alias}</span>
		{/if}
		<button
			type="button"
			class="nodrag ml-auto grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-hover hover:text-foreground"
			aria-label="Remove {data.alias}"
			onclick={data.onremove}><XIcon class="size-3.5" /></button
		>
	</div>
	<div class="nowheel max-h-72 overflow-y-auto py-1 text-xs">
		{#if data.loading && data.columns.length === 0}
			<div class="flex items-center gap-1.5 px-2.5 py-1 text-muted-foreground">
				<LoaderIcon class="size-3 animate-spin" /> Loading columns
			</div>
		{/if}
		{#each data.columns as column (column.name)}
			<div
				class="relative flex h-6 items-center gap-1.5 pr-2.5 pl-2.5 {chosen.has(column.name)
					? 'bg-signal-soft/60'
					: 'hover:bg-hover'}"
			>
				<Checkbox
					class="nodrag relative z-10"
					checked={chosen.has(column.name)}
					aria-label="Select {data.alias}.{column.name}"
					onCheckedChange={() => data.ontoggle(column.name)}
				/>
				<span class="flex w-3 shrink-0 justify-center">
					{#if column.primaryKey}<KeyRoundIcon class="size-3 text-ink-2" />{/if}
				</span>
				<span class="measured truncate {column.primaryKey ? 'text-foreground' : 'text-ink-2'}"
					>{column.name}</span
				>
				<span class="measured ml-auto shrink-0 truncate pl-2 text-muted-foreground"
					>{column.dataType}</span
				>
				<!-- Anchor only: lines end on the left edge when the other table sits to the left. -->
				<Handle
					type="source"
					position={Position.Left}
					id={leftHandle(column.name)}
					class="builder-handle !pointer-events-none !left-0"
				/>
				<!-- The whole row past the checkbox is the drag source for a join. -->
				<Handle
					type="source"
					position={Position.Right}
					id={rightHandle(column.name)}
					class="builder-handle !right-0 !left-7 cursor-crosshair"
					title="Drag onto a column of another table to join"
				/>
			</div>
		{/each}
	</div>
</div>

<style>
	:global(.svelte-flow .builder-handle) {
		position: absolute;
		top: 0;
		bottom: 0;
		height: auto;
		width: auto;
		min-width: 0;
		min-height: 0;
		transform: none;
		border: 0;
		border-radius: 0;
		background: transparent;
	}
	:global(.svelte-flow .builder-handle.connectingto),
	:global(.svelte-flow .builder-handle.valid) {
		background: color-mix(in oklab, var(--color-accent) 22%, transparent);
	}
</style>
