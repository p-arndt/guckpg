<script lang="ts">
	import * as Select from '#lib/components/ui/select/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { checkExpression } from '#lib/structure.js';

	let {
		columns,
		disabled = false,
		onapply
	}: { columns: string[]; disabled?: boolean; onapply: (expression: string) => void } = $props();
	let column = $state(''),
		operator = $state('>='),
		value = $state('0');
	const expression = $derived(checkExpression(column, operator, value));
	const operators = ['>=', '>', '<=', '<', '=', '<>'];
</script>

<div class="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
	<span>Quick condition</span>
	<Select.Root type="single" bind:value={column} {disabled}>
		<Select.Trigger size="sm" class="min-w-28" aria-label="Column">
			{column || 'Column'}
		</Select.Trigger>
		<Select.Content>
			{#each columns as name (name)}<Select.Item value={name} label={name} />{/each}
		</Select.Content>
	</Select.Root>
	<Select.Root type="single" bind:value={operator} {disabled}>
		<Select.Trigger size="sm" class="measured" aria-label="Operator">{operator}</Select.Trigger>
		<Select.Content>
			{#each operators as op (op)}<Select.Item value={op} label={op} class="measured" />{/each}
		</Select.Content>
	</Select.Root>
	<input
		class="measured h-7 w-24 rounded-md border border-input bg-background px-2 text-xs text-foreground"
		bind:value
		aria-label="Value"
		placeholder="0"
		{disabled}
	/>
	<Button
		size="xs"
		variant="outline"
		disabled={disabled || !expression}
		onclick={() => expression && onapply(expression)}>Use</Button
	>
</div>
