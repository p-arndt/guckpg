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
		icon?: Snippet;
		trailing?: Snippet;
		label: string;
	}

	let {
		depth,
		expanded,
		loading = false,
		icon,
		trailing,
		label,
		class: className,
		...rest
	}: Props = $props();
</script>

<button
	{...rest}
	type="button"
	class={cn(
		'group flex h-7 w-full items-center gap-1 rounded-md pr-2 text-left text-[13px] hover:bg-sidebar-accent focus-visible:bg-sidebar-accent focus-visible:outline-none data-[state=open]:bg-sidebar-accent',
		className
	)}
	style:padding-left="{depth * 12 + 4}px"
	title={label}
>
	<span class="flex size-4 shrink-0 items-center justify-center text-muted-foreground">
		{#if loading}
			<LoaderIcon class="size-3.5 animate-spin" />
		{:else if expanded !== undefined}
			<ChevronRightIcon class="size-3.5 transition-transform {expanded ? 'rotate-90' : ''}" />
		{/if}
	</span>
	{#if icon}<span class="flex shrink-0 items-center text-muted-foreground">{@render icon()}</span
		>{/if}
	<span class="min-w-0 flex-1 truncate">{label}</span>
	{#if trailing}{@render trailing()}{/if}
</button>
