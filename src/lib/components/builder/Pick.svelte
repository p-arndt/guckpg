<script lang="ts">
	import * as Select from '#lib/components/ui/select/index.js';
	import { cn } from '#lib/utils.js';

	interface Props {
		value: string;
		options: { value: string; label: string }[];
		placeholder?: string;
		label: string;
		class?: string;
		onchange: (value: string) => void;
	}

	let {
		value,
		options,
		placeholder = 'Choose',
		label,
		class: className,
		onchange
	}: Props = $props();

	const current = $derived(options.find((option) => option.value === value));
</script>

<Select.Root type="single" {value} onValueChange={onchange}>
	<Select.Trigger
		size="sm"
		aria-label={label}
		class={cn(
			'h-7 min-w-0 gap-1 border-transparent bg-transparent px-2 text-xs shadow-none hover:border-rule-2',
			!current && 'text-muted-foreground',
			className
		)}
	>
		<span class="truncate">{current?.label ?? placeholder}</span>
	</Select.Trigger>
	<Select.Content class="max-h-72">
		{#each options as option (option.value)}
			<Select.Item value={option.value} label={option.label} />
		{/each}
	</Select.Content>
</Select.Root>
