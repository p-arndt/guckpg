<script lang="ts">
	import type { Snippet } from 'svelte';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import { cn } from '#lib/utils.js';

	interface Props extends HTMLButtonAttributes {
		depth: number;
		/** undefined for leaves */
		expanded?: boolean;
		loading?: boolean;
		showChevron?: boolean;
		icon?: Snippet;
		trailing?: Snippet;
		label: string;
	}

	let {
		depth,
		expanded,
		loading = false,
		showChevron = true,
		icon,
		trailing,
		label,
		class: className,
		...rest
	}: Props = $props();
</script>

<button
	title={label}
	{...rest}
	type="button"
	role="treeitem"
	aria-level={depth + 1}
	aria-expanded={expanded}
	aria-busy={loading || undefined}
	class={cn(
		'group flex h-[26px] w-full shrink-0 items-center gap-1.5 rounded-lg pr-2 text-left text-[13px] text-ink-2 transition-[background-color,color] duration-(--dur-micro) hover:bg-hover hover:text-foreground data-[state=open]:bg-hover data-[state=open]:text-foreground',
		className
	)}
	style:padding-left="{depth * 10 +
		(loading || (showChevron && expanded !== undefined) ? 4 : 22)}px"
>
	{#if loading || (showChevron && expanded !== undefined)}
		<span class="flex size-4 shrink-0 items-center justify-center text-muted-foreground">
			{#if loading}
				<LoaderIcon class="size-3.5 animate-spin" />
			{:else if expanded !== undefined}
				<ChevronRightIcon
					class="size-3.5 transition-transform duration-(--dur-micro) {expanded ? 'rotate-90' : ''}"
				/>
			{/if}
		</span>
	{/if}
	{#if icon}<span class="flex shrink-0 items-center text-muted-foreground">{@render icon()}</span
		>{/if}
	<span class="min-w-0 flex-1 truncate">{label}</span>
	{#if trailing}<span class="flex shrink-0 items-center gap-1.5 whitespace-nowrap"
			>{@render trailing()}</span
		>{/if}
</button>
