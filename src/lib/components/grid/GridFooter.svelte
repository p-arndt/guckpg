<script lang="ts">
	import PlusIcon from '@lucide/svelte/icons/plus';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import UndoIcon from '@lucide/svelte/icons/undo-2';
	import RedoIcon from '@lucide/svelte/icons/redo-2';
	import CodeIcon from '@lucide/svelte/icons/code';
	import { Button } from '#lib/components/ui/button/index.js';
	import type { SourceInfo } from '#lib/api.js';
	import type { GridEdits } from '#lib/state/edits.svelte.js';

	interface Props {
		edits: GridEdits;
		editable: boolean;
		source: SourceInfo | null;
		editTarget: { readOnly: boolean } | null;
		saving: boolean;
		onAddRow: () => void;
		onPreview: () => void;
		onSave: () => void;
	}
	let { edits, editable, source, editTarget, saving, onAddRow, onPreview, onSave }: Props =
		$props();
</script>

{#if editable && source}
	<div class="flex h-9 shrink-0 items-center gap-1 border-t px-2 text-xs">
		<Button size="xs" variant="ghost" onclick={onAddRow} title="Add a row">
			<PlusIcon /> Row
		</Button>
		<Button
			size="icon-xs"
			variant="ghost"
			disabled={!edits.canUndo}
			onclick={() => edits.undo()}
			title="Undo (⌘Z)"
		>
			<UndoIcon />
		</Button>
		<Button
			size="icon-xs"
			variant="ghost"
			disabled={!edits.canRedo}
			onclick={() => edits.redo()}
			title="Redo (⇧⌘Z)"
		>
			<RedoIcon />
		</Button>
		{#if edits.count > 0}
			<span class="ml-auto font-medium text-signal-text">
				<span class="measured">{edits.count}</span> unsaved change{edits.count === 1 ? '' : 's'}
			</span>
			<Button size="xs" variant="ghost" onclick={onPreview} title="Show the SQL that will run">
				<CodeIcon /> SQL
			</Button>
			<Button size="xs" variant="ghost" disabled={saving} onclick={() => edits.discard()}>
				Discard
			</Button>
			<Button size="xs" disabled={saving} onclick={onSave}>
				{#if saving}<LoaderIcon class="animate-spin" />{/if}
				Save <span class="measured opacity-70">⌘S</span>
			</Button>
		{:else}
			<span class="ml-auto flex min-w-0 items-center gap-4 truncate text-muted-foreground">
				<span class="truncate">Double-click or type to edit</span>
				<span class="flex items-center gap-1.5 whitespace-nowrap"
					><kbd class="key">⌫</kbd> deletes a row</span
				>
				<span class="flex items-center gap-1.5 whitespace-nowrap"
					><kbd class="key">⌘V</kbd> pastes from spreadsheets</span
				>
				<span class="measured whitespace-nowrap text-ink-2">{source.schema}.{source.table}</span>
			</span>
		{/if}
	</div>
{:else if editTarget && !source}
	<div class="flex h-8 shrink-0 items-center border-t px-3 text-xs text-muted-foreground">
		Read-only result: editing needs rows from one table with its primary key in the result.
	</div>
{:else if editTarget?.readOnly && source}
	<div class="flex h-8 shrink-0 items-center border-t px-3 text-xs text-muted-foreground">
		Read-only connection.
	</div>
{/if}
