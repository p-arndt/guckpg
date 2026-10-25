<script lang="ts">
	import XIcon from '@lucide/svelte/icons/x';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import TableIcon from '@lucide/svelte/icons/table-2';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import NetworkIcon from '@lucide/svelte/icons/network';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { tick } from 'svelte';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs, type Tab } from '#lib/state/tabs.svelte.js';

	interface Props {
		onNewQuery: () => void;
	}

	let { onNewQuery }: Props = $props();

	let renaming = $state<string | null>(null);
	let renameValue = $state('');
	let dragging = $state<string | null>(null);
	let dropIndex = $state<number | null>(null);

	async function startRename(tab: Tab) {
		if (tab.kind !== 'query') return;
		renaming = tab.id;
		renameValue = tab.title;
		await tick();
		const input = document.getElementById(`rename-${tab.id}`) as HTMLInputElement | null;
		input?.focus();
		input?.select();
	}

	function finishRename(save: boolean) {
		if (renaming && save) tabs.rename(renaming, renameValue);
		renaming = null;
	}

	function onDragOver(e: DragEvent, index: number) {
		if (!dragging) return;
		e.preventDefault();
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		dropIndex = e.clientX < rect.left + rect.width / 2 ? index : index + 1;
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		if (dragging && dropIndex !== null) {
			const from = tabs.items.findIndex((t) => t.id === dragging);
			tabs.move(dragging, dropIndex > from ? dropIndex - 1 : dropIndex);
		}
		dragging = null;
		dropIndex = null;
	}

	const modKey = typeof navigator !== 'undefined' && /Mac/.test(navigator.platform) ? '⌘' : 'Ctrl+';
</script>

<div
	class="flex h-9 shrink-0 items-end gap-px overflow-x-auto border-b bg-muted/40 px-1"
	role="tablist"
	tabindex="-1"
	ondragover={(e) => dragging && e.preventDefault()}
	ondrop={onDrop}
>
	{#each tabs.items as tab, index (tab.id)}
		{@const color = connections.get(tab.connectionId)?.color}
		{@const active = tab.id === tabs.activeId}
		<ContextMenu.Root>
			<ContextMenu.Trigger>
				{#snippet child({ props })}
					<div
						{...props}
						role="presentation"
						draggable={renaming !== tab.id}
						ondragstart={(e) => {
							dragging = tab.id;
							e.dataTransfer?.setData('text/plain', tab.title);
							if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
						}}
						ondragend={() => {
							dragging = null;
							dropIndex = null;
						}}
						ondragover={(e) => onDragOver(e, index)}
						class="group relative flex h-8 max-w-56 shrink-0 items-center gap-1.5 rounded-t-md border border-b-0 pr-1 pl-2.5 text-xs {active
							? 'bg-background text-foreground'
							: 'border-transparent text-muted-foreground hover:bg-background/60'} {dragging ===
						tab.id
							? 'opacity-50'
							: ''}"
					>
						{#if dropIndex === index && dragging !== tab.id}
							<span class="absolute inset-y-1 -left-px w-0.5 rounded-full bg-primary"></span>
						{:else if dropIndex === index + 1 && index === tabs.items.length - 1}
							<span class="absolute inset-y-1 -right-px w-0.5 rounded-full bg-primary"></span>
						{/if}
						{#if color}
							<span
								class="absolute inset-x-0 top-0 h-0.5 rounded-t-md"
								style:background-color={color}
							></span>
						{/if}
						{#if renaming === tab.id}
							<SquareTerminalIcon class="size-3.5 shrink-0" />
							<input
								id="rename-{tab.id}"
								bind:value={renameValue}
								class="h-5 w-32 rounded border bg-background px-1 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
								spellcheck="false"
								onblur={() => finishRename(true)}
								onkeydown={(e) => {
									if (e.key === 'Enter') finishRename(true);
									else if (e.key === 'Escape') finishRename(false);
								}}
							/>
						{:else}
							<button
								role="tab"
								aria-selected={active}
								class="flex min-w-0 items-center gap-1.5"
								onclick={() => (tabs.activeId = tab.id)}
								ondblclick={() => startRename(tab)}
								onauxclick={(e) => {
									if (e.button === 1) void tabs.requestClose(tab.id);
								}}
								title="{tab.title} · {tab.database}{tab.kind === 'query'
									? ' · double-click to rename'
									: ''}"
							>
								{#if tab.running}
									<LoaderIcon class="size-3.5 shrink-0 animate-spin" />
								{:else if tab.kind === 'table'}
									<TableIcon class="size-3.5 shrink-0" />
								{:else if tab.kind === 'diagram'}
									<NetworkIcon class="size-3.5 shrink-0" />
								{:else}
									<SquareTerminalIcon class="size-3.5 shrink-0" />
								{/if}
								<span class="truncate">{tab.title}</span>
								{#if tab.dirty}
									<span class="size-1.5 shrink-0 rounded-full bg-amber-500" title="Unsaved changes"
									></span>
								{/if}
							</button>
						{/if}
						<button
							class="rounded p-0.5 opacity-0 group-hover:opacity-100 hover:bg-accent {active
								? 'opacity-60'
								: ''}"
							title="Close ({modKey}W)"
							onclick={() => tabs.requestClose(tab.id)}
						>
							<XIcon class="size-3" />
						</button>
					</div>
				{/snippet}
			</ContextMenu.Trigger>
			<ContextMenu.Content class="w-56">
				{#if tab.kind === 'query'}
					<ContextMenu.Item onSelect={() => startRename(tab)}>Rename…</ContextMenu.Item>
					<ContextMenu.Item onSelect={() => tabs.duplicate(tab.id)}>Duplicate</ContextMenu.Item>
					<ContextMenu.Separator />
				{/if}
				<ContextMenu.Item onSelect={() => tabs.requestClose(tab.id)}>
					Close <ContextMenu.Shortcut>{modKey}W</ContextMenu.Shortcut>
				</ContextMenu.Item>
				<ContextMenu.Item
					disabled={tabs.items.length < 2}
					onSelect={() => tabs.closeOthers(tab.id)}
				>
					Close others
				</ContextMenu.Item>
				<ContextMenu.Item
					disabled={index === tabs.items.length - 1}
					onSelect={() => tabs.closeToRight(tab.id)}
				>
					Close to the right
				</ContextMenu.Item>
				<ContextMenu.Item onSelect={() => tabs.closeAll()}>Close all</ContextMenu.Item>
				<ContextMenu.Separator />
				<ContextMenu.Item
					disabled={!tabs.closed.length}
					onSelect={() => tabs.reopenClosed((id) => !!connections.get(id))}
				>
					Reopen closed tab <ContextMenu.Shortcut>{modKey}⇧T</ContextMenu.Shortcut>
				</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Root>
	{/each}
	<button
		class="mb-1 ml-1 shrink-0 self-center rounded p-1 text-muted-foreground hover:bg-background/60 hover:text-foreground"
		title="New query ({modKey}T)"
		onclick={onNewQuery}
	>
		<PlusIcon class="size-3.5" />
	</button>
</div>
