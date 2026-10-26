<script lang="ts">
	import * as ResizablePrimitive from 'paneforge';
	import { cn, type WithoutChildrenOrChild } from '#lib/utils.js';

	let {
		ref = $bindable(null),
		class: className,
		withHandle = false,
		variant = 'line',
		...restProps
	}: WithoutChildrenOrChild<ResizablePrimitive.PaneResizerProps> & {
		withHandle?: boolean;
		/** `gap` is an invisible gutter between floating panels; `line` is a hairline. */
		variant?: 'line' | 'gap';
	} = $props();
</script>

<ResizablePrimitive.PaneResizer
	bind:ref
	data-slot="resizable-handle"
	class={cn(
		'relative flex w-px items-center justify-center bg-border transition-[background-color] duration-(--dur-micro) after:absolute after:inset-y-0 after:left-1/2 after:w-1 after:-translate-x-1/2 hover:bg-rule-2 focus-visible:outline-hidden data-[active]:bg-signal data-[direction=vertical]:h-px data-[direction=vertical]:w-full data-[direction=vertical]:after:left-0 data-[direction=vertical]:after:h-1 data-[direction=vertical]:after:w-full data-[direction=vertical]:after:translate-x-0 data-[direction=vertical]:after:-translate-y-1/2 [&[data-direction=vertical]>div]:rotate-90',
		variant === 'gap' &&
			'w-2 bg-transparent hover:bg-transparent data-[active]:bg-transparent data-[direction=vertical]:h-2',
		className
	)}
	{...restProps}
>
	{#if withHandle}
		<div class="z-10 flex h-6 w-1 shrink-0 rounded-lg bg-border"></div>
	{/if}
</ResizablePrimitive.PaneResizer>
