<script lang="ts">
	import { parsePlan, type PlanNode } from '#lib/explain.js';
	interface Props {
		value: string | null | undefined;
	}
	let { value }: Props = $props();
	const plan = $derived(parsePlan(value));
</script>

{#snippet node(item: PlanNode)}
	<li class="border-l pl-4">
		<div class="my-2 rounded-lg border bg-chrome p-3 text-sm">
			<div class="font-medium">{item['Node Type']}</div>
			{#if item['Relation Name']}<div class="text-muted-foreground">
					{item['Relation Name']}
				</div>{/if}
			{#if item['Index Name']}<div class="text-muted-foreground">
					Index: {item['Index Name']}
				</div>{/if}
			<div class="mt-1 text-xs text-muted-foreground">
				{#if item['Plan Rows'] !== undefined}Estimated rows: {item[
						'Plan Rows'
					].toLocaleString()}{/if}
				{#if item['Total Cost'] !== undefined}
					· Cost: {item['Total Cost'].toLocaleString()}{/if}
			</div>
		</div>
		{#if item.Plans?.length}<ul>
				{#each item.Plans as child (child)}{@render node(child)}{/each}
			</ul>{/if}
	</li>
{/snippet}

<div class="h-full overflow-auto p-4">
	{#if plan}
		<p class="mb-2 text-xs text-muted-foreground">Estimated plan · query was not executed</p>
		<ul aria-label="Query plan">{@render node(plan)}</ul>
	{:else}
		<p class="text-sm text-muted-foreground">Could not read the query plan.</p>
	{/if}
</div>
