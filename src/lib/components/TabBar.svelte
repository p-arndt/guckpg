<script lang="ts">
	import { shortcut } from '#lib/shortcut.js';
	import XIcon from '@lucide/svelte/icons/x';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import TableIcon from '@lucide/svelte/icons/table-2';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import NetworkIcon from '@lucide/svelte/icons/network';
	import UserIcon from '@lucide/svelte/icons/user';
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
	let strip = $state<HTMLDivElement | null>(null);

	// The strip has no visible scrollbar, so keep the active tab in view.
	$effect(() => {
		const id = tabs.activeId;
		if (!strip || !id) return;
		strip
			.querySelector(`[data-tab-id="${CSS.escape(id)}"]`)
			?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
	});

	/** A vertical wheel scrolls the strip sideways. */
	function onWheel(e: WheelEvent) {
		if (!strip || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
		strip.scrollLeft += e.deltaY;
	}

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
</script>

<div
	class="flex h-10 shrink-0 scroll-px-1.5 scroll-pe-9 [scrollbar-width:none] items-center gap-0.5 overflow-x-auto overflow-y-hidden border-b px-1.5 [&::-webkit-scrollbar]:hidden"
	bind:this={strip}
	onwheel={onWheel}
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
						data-tab-id={tab.id}
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
						class="group relative flex h-7 max-w-60 shrink-0 items-center gap-1 rounded-lg pr-0.5 pl-2.5 text-[13px] transition-[background-color,color] duration-(--dur-micro) {active
							? 'bg-hover text-foreground'
							: 'text-muted-foreground hover:bg-hover/60 hover:text-foreground'} {dragging ===
						tab.id
							? 'opacity-50'
							: ''}"
					>
						{#if dropIndex === index && dragging !== tab.id}
							<span class="absolute inset-y-1 -left-0.5 w-0.5 rounded-full bg-signal"></span>
						{:else if dropIndex === index + 1 && index === tabs.items.length - 1}
							<span class="absolute inset-y-1 -right-0.5 w-0.5 rounded-full bg-signal"></span>
						{/if}
						{#if renaming === tab.id}
							<SquareTerminalIcon class="size-3.5 shrink-0" />
							<input
								id="rename-{tab.id}"
								bind:value={renameValue}
								aria-label="Tab name"
								class="h-6 w-36 rounded-md border border-input bg-background px-1.5 text-[13px]"
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
								class="flex h-full min-w-0 items-center gap-1.5 rounded-sm"
								onclick={() => (tabs.activeId = tab.id)}
								ondblclick={() => startRename(tab)}
								onauxclick={(e) => {
									if (e.button === 1) void tabs.requestClose(tab.id);
								}}
								title="{tab.title} in {tab.database}{tab.kind === 'query'
									? ', double-click to rename'
									: ''}"
							>
								{#if tab.running}
									<LoaderIcon class="size-3.5 shrink-0 animate-spin text-cool" />
								{:else if tab.kind === 'table'}
									<TableIcon class="size-3.5 shrink-0" style={color ? `color: ${color}` : ''} />
								{:else if tab.kind === 'role'}
									<UserIcon class="size-3.5 shrink-0" style={color ? `color: ${color}` : ''} />
								{:else if tab.kind === 'diagram'}
									<NetworkIcon class="size-3.5 shrink-0" style={color ? `color: ${color}` : ''} />
								{:else}
									<SquareTerminalIcon
										class="size-3.5 shrink-0"
										style={color ? `color: ${color}` : ''}
									/>
								{/if}
								<span class="truncate">{tab.title}</span>
								{#if tab.dirty}
									<span class="size-1.5 shrink-0 rounded-full bg-signal" title="Unsaved changes"
									></span>
								{/if}
							</button>
						{/if}
						<button
							type="button"
							aria-label="Close {tab.title}"
							class="flex size-5 items-center justify-center rounded-md text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-foreground focus-visible:opacity-100 {active
								? 'opacity-100'
								: ''}"
							title="Close ({shortcut('W')})"
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
					Close <ContextMenu.Shortcut>{shortcut('W')}</ContextMenu.Shortcut>
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
					Reopen closed tab <ContextMenu.Shortcut>{shortcut('T', true)}</ContextMenu.Shortcut>
				</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Root>
	{/each}
	<button
		type="button"
		aria-label="New query"
		class="sticky right-0 flex size-7 shrink-0 items-center justify-center rounded-lg bg-background text-muted-foreground transition-[background-color,color] duration-(--dur-micro) hover:bg-hover hover:text-foreground"
		title="New query ({shortcut('T')})"
		onclick={onNewQuery}
	>
		<PlusIcon class="size-3.5" />
	</button>
</div>
