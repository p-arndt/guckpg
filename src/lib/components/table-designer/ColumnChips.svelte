<script lang="ts">
	interface Props {
		/** Every column of the draft, in table order. */
		columns: string[];
		selected: string[];
		toggle: (name: string) => void;
		/** Numbers the picks once there is more than one, for keys where order matters. */
		ordered?: boolean;
		disabled?: boolean;
	}
	let { columns, selected, toggle, ordered = false, disabled = false }: Props = $props();
</script>

<div class="flex flex-wrap gap-1.5">
	{#each columns as name (name)}
		{@const at = selected.indexOf(name)}
		<button
			type="button"
			class="chip-toggle"
			aria-pressed={at >= 0}
			{disabled}
			onclick={() => toggle(name)}
		>
			{#if ordered && at >= 0 && selected.length > 1}<span class="measured order">{at + 1}</span
				>{/if}{name}
		</button>
	{:else}
		<span class="text-xs text-muted-foreground">Add columns first.</span>
	{/each}
</div>

<style>
	.chip-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		height: 1.5rem;
		padding-inline: 0.5rem;
		border: 1px solid var(--color-rule-2);
		border-radius: var(--radius-pill);
		font-family: var(--font-outlier);
		font-size: 0.75rem;
		color: var(--color-muted);
		transition:
			background-color var(--dur-micro),
			color var(--dur-micro);
	}
	.chip-toggle:hover:not(:disabled) {
		background: var(--color-hover);
		color: var(--color-ink);
	}
	.chip-toggle[aria-pressed='true'] {
		background: var(--color-accent-soft);
		border-color: color-mix(in oklab, var(--color-accent) 55%, transparent);
		color: var(--color-ink);
	}
	.order {
		display: grid;
		place-items: center;
		min-width: 1rem;
		height: 1rem;
		border-radius: 999px;
		background: var(--color-accent);
		color: var(--color-accent-ink);
		font-size: 0.625rem;
	}
</style>
