<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { confirmDiscard } from '#lib/state/confirm.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import TableDesigner from './TableDesigner.svelte';

	let {
		open = $bindable(false),
		connectionId,
		database,
		schema
	}: {
		open: boolean;
		connectionId: string;
		database: string;
		schema: string;
	} = $props();
	let dirty = $state(0);
	let busy = $state(false);
	async function changeOpen(next: boolean) {
		if (busy) return;
		if (!next && !(await confirmDiscard(dirty))) return;
		open = next;
	}
	function created(name: string) {
		dirty = 0;
		open = false;
		connections.refresh(connectionId);
		tabs.openTable(connectionId, database, schema, name).view = 'structure';
	}
</script>

<Dialog.Root {open} onOpenChange={changeOpen}>
	<Dialog.Content
		class="flex h-[85vh] w-[min(1100px,95vw)] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none"
		showCloseButton={!busy}
	>
		<!-- The designer brings its own header and footer bands, so it runs edge to edge below this. -->
		<Dialog.Header class="gap-1 border-b px-5 pt-4 pb-3.5">
			<Dialog.Title>Create table</Dialog.Title>
			<Dialog.Description>
				In <span class="measured text-ink-2">{schema}</span>. You review the SQL before it runs.
			</Dialog.Description>
		</Dialog.Header>
		{#if open}
			<div class="min-h-0 flex-1">
				<TableDesigner
					{connectionId}
					{database}
					{schema}
					onApplied={created}
					onDirtyChange={(n) => (dirty = n)}
					onBusyChange={(value) => (busy = value)}
				/>
			</div>
		{/if}
	</Dialog.Content>
</Dialog.Root>
