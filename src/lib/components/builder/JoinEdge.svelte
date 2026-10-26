<script lang="ts" module>
	import type { JoinType } from '#lib/query-builder.js';

	export interface JoinEdgeData extends Record<string, unknown> {
		type: JoinType;
		fromAlias: string;
		toAlias: string;
		onchange: (type: JoinType) => void;
		onremove: () => void;
	}

	/** What a join keeps, in words, because LEFT/RIGHT depend on which end is which. */
	export function joinLabel(type: JoinType, fromAlias: string, toAlias: string): string {
		if (type === 'INNER') return 'matching rows';
		if (type === 'LEFT') return `all ${fromAlias}`;
		if (type === 'RIGHT') return `all ${toAlias}`;
		return 'all rows from both';
	}
</script>

<script lang="ts">
	import { BaseEdge, EdgeLabel, getBezierPath, type Edge, type EdgeProps } from '@xyflow/svelte';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import { JOIN_TYPES } from '#lib/query-builder.js';

	let {
		sourceX,
		sourceY,
		targetX,
		targetY,
		sourcePosition,
		targetPosition,
		data,
		selected
	}: EdgeProps<Edge<JoinEdgeData, 'join'>> = $props();

	const geometry = $derived(
		getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition })
	);
</script>

<BaseEdge
	path={geometry[0]}
	interactionWidth={16}
	style="stroke: var(--color-{selected ? 'accent' : 'cool'}); stroke-width: {selected ? 2 : 1.5}px"
/>
{#if data}
	<EdgeLabel x={geometry[1]} y={geometry[2]} class="nodrag nopan">
		<DropdownMenu.Root>
			<DropdownMenu.Trigger>
				{#snippet child({ props })}
					<button
						{...props}
						type="button"
						class="measured rounded-md border border-rule-2 bg-raised px-1.5 py-0.5 text-[10px] text-ink-2 shadow-panel hover:border-cool hover:text-foreground"
						title="{joinLabel(data.type, data.fromAlias, data.toAlias)}. Click to change."
					>
						{data.type.toLowerCase()}
					</button>
				{/snippet}
			</DropdownMenu.Trigger>
			<DropdownMenu.Content class="w-60">
				<DropdownMenu.RadioGroup
					value={data.type}
					onValueChange={(value) => data.onchange(value as JoinType)}
				>
					{#each JOIN_TYPES as type (type)}
						<DropdownMenu.RadioItem value={type}>
							<span class="flex-1">{joinLabel(type, data.fromAlias, data.toAlias)}</span>
							<span class="measured text-xs text-muted-foreground">{type}</span>
						</DropdownMenu.RadioItem>
					{/each}
				</DropdownMenu.RadioGroup>
				<DropdownMenu.Separator />
				<DropdownMenu.Item variant="destructive" onSelect={data.onremove}
					>Remove join</DropdownMenu.Item
				>
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	</EdgeLabel>
{/if}
