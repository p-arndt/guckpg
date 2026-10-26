<script lang="ts">
	import type { ClassValue } from 'svelte/elements';
	import type { ObjectPrivileges } from '#lib/api.js';
	import { grantorsOf, objectKey, type Grants } from '#lib/role-editor.js';

	interface Props {
		/** Header of the name column, e.g. "Database". */
		heading: string;
		objects: ObjectPrivileges[];
		privileges: readonly string[];
		grants: Grants;
		original: Grants;
		locked: boolean;
		onToggle: (object: ObjectPrivileges, privilege: string) => void;
		/** Turns the privilege headers into buttons that toggle the whole column. */
		onToggleAll?: (privilege: string) => void;
		/** Shown as the only row when `objects` is empty. */
		empty?: string;
		class?: ClassValue;
	}
	let {
		heading,
		objects,
		privileges,
		grants,
		original,
		locked,
		onToggle,
		onToggleAll,
		empty,
		class: className = 'w-full table-fixed text-[13px]'
	}: Props = $props();

	const LABELS: Record<string, string> = {
		TEMPORARY: 'Temp',
		REFERENCES: 'References'
	};
	const label = (privilege: string) =>
		LABELS[privilege] ?? privilege.charAt(0) + privilege.slice(1).toLowerCase();
</script>

{#snippet grantCell(object: ObjectPrivileges, key: string, privilege: string)}
	{@const granted = grants[key]?.includes(privilege) ?? false}
	{@const originallyGranted = original[key]?.includes(privilege) ?? false}
	{@const changed = granted !== originallyGranted}
	{@const grantors = granted && originallyGranted ? grantorsOf(object, privilege) : []}
	{@const inherited =
		!object.owner && !granted && !originallyGranted && object.effective.includes(privilege)}
	<td
		class="px-2 py-1 text-center"
		class:inherited
		class:bg-signal-soft={changed}
		title={object.owner
			? 'Owners have every privilege'
			: inherited
				? 'Allowed through PUBLIC or a member role'
				: grantors.length
					? `Granted by ${grantors.join(', ')}`
					: undefined}
	>
		<input
			type="checkbox"
			class="grant"
			checked={granted || object.owner}
			disabled={locked || object.owner}
			aria-label="{privilege} on {object.schema ? `${object.schema}.` : ''}{object.name}"
			onchange={() => onToggle(object, privilege)}
		/>
	</td>
{/snippet}

<table class={className}>
	<thead class="bg-chrome text-xs text-muted-foreground">
		<tr class="border-b">
			<th class="py-1.5 pl-4 text-left font-normal">{heading}</th>
			{#each privileges as privilege (privilege)}
				{#if onToggleAll}
					<th class="w-24 py-1 font-normal">
						<button
							type="button"
							class="rounded px-1.5 py-0.5 hover:bg-hover hover:text-foreground disabled:pointer-events-none"
							title="Toggle {privilege} on all {objects.length} shown tables"
							disabled={locked || !objects.length}
							onclick={() => onToggleAll(privilege)}>{label(privilege)}</button
						>
					</th>
				{:else}
					<th class="w-24 py-1.5 font-normal">{label(privilege)}</th>
				{/if}
			{/each}
		</tr>
	</thead>
	<tbody>
		{#each objects as object (objectKey(object))}
			{@const key = objectKey(object)}
			<tr class="border-b last:border-b-0 hover:bg-hover">
				<td class="max-w-0 py-1 pr-3 pl-4">
					<div class="flex min-w-0 items-center gap-2">
						<span class="measured truncate"
							>{#if object.schema}<span class="text-muted-foreground">{object.schema}.</span
								>{/if}{object.name}</span
						>
						{#if object.kind !== 'table' && object.kind !== 'database' && object.kind !== 'schema'}
							<span class="text-[11px] text-muted-foreground">{object.kind}</span>
						{/if}
						{#if object.owner}<span class="chip">owner</span>{/if}
					</div>
				</td>
				{#each privileges as privilege (privilege)}
					{@render grantCell(object, key, privilege)}
				{/each}
			</tr>
		{:else}
			{#if empty}
				<tr>
					<td colspan={privileges.length + 1} class="px-4 py-2.5 text-muted-foreground">{empty}</td>
				</tr>
			{/if}
		{/each}
	</tbody>
</table>

<style>
	.grant {
		width: 0.875rem;
		height: 0.875rem;
		accent-color: var(--color-action);
		cursor: pointer;
		vertical-align: middle;
	}
	.inherited {
		background: color-mix(in oklab, var(--color-ok) 18%, transparent);
	}
	.grant:disabled {
		cursor: default;
		opacity: 0.55;
	}
</style>
