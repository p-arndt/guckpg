<script lang="ts" module>
	/** What the menu acts on: whole picked rows, or the selected cell. */
	export type GridMenuTarget =
		| { kind: 'rows'; count: number; allDeleted: boolean }
		| {
				kind: 'cell';
				selected: boolean;
				canEdit: boolean;
				canRevert: boolean;
				rowDeleted: boolean;
		  };

	export interface GridMenuActions {
		openRowForm: () => void;
		copyRows: (withHeader: boolean) => void;
		toggleDelete: () => void;
		addRow: () => void;
		editCell: () => void;
		setNull: () => void;
		revertCell: () => void;
		undo: () => void;
		copyValue: () => void;
		copyRow: () => void;
		openValue: () => void;
		copyColumnName: () => void;
		copyAll: () => void;
	}
</script>

<script lang="ts">
	import { shortcut } from '#lib/shortcut.js';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';

	interface Props {
		target: GridMenuTarget;
		actions: GridMenuActions;
		editable: boolean;
		saving: boolean;
		canUndo: boolean;
	}
	let { target, actions, editable, saving, canUndo }: Props = $props();
</script>

<ContextMenu.Content class="w-56">
	{#if target.kind === 'rows'}
		{@const what = target.count === 1 ? 'row' : `${target.count} rows`}
		<ContextMenu.Item disabled={target.count !== 1 || saving} onSelect={actions.openRowForm}>
			Open row form<ContextMenu.Shortcut>⇧↵</ContextMenu.Shortcut>
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={() => actions.copyRows(false)}>
			Copy {what}
			<ContextMenu.Shortcut>{shortcut('C')}</ContextMenu.Shortcut>
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={() => actions.copyRows(true)}>
			Copy {what} with header
		</ContextMenu.Item>
		{#if editable}
			<ContextMenu.Separator />
			<ContextMenu.Item disabled={saving} onSelect={actions.toggleDelete}>
				{target.allDeleted ? 'Restore' : 'Delete'}
				{what}
				<ContextMenu.Shortcut>⌫</ContextMenu.Shortcut>
			</ContextMenu.Item>
			<ContextMenu.Item onSelect={actions.addRow}>Add row</ContextMenu.Item>
		{/if}
	{:else}
		<ContextMenu.Item disabled={!target.selected || saving} onSelect={actions.openRowForm}>
			Open row form<ContextMenu.Shortcut>⇧↵</ContextMenu.Shortcut>
		</ContextMenu.Item>
		<ContextMenu.Separator />
		{#if editable}
			<ContextMenu.Item disabled={!target.canEdit} onSelect={actions.editCell}>
				Edit cell
				<ContextMenu.Shortcut>↵</ContextMenu.Shortcut>
			</ContextMenu.Item>
			<ContextMenu.Item disabled={!target.canEdit} onSelect={actions.setNull}>
				Set NULL
			</ContextMenu.Item>
			<ContextMenu.Item disabled={!target.canRevert} onSelect={actions.revertCell}>
				Revert cell
			</ContextMenu.Item>
			<ContextMenu.Item disabled={!target.selected} onSelect={actions.toggleDelete}>
				{target.rowDeleted ? 'Restore row' : 'Delete row'}
				<ContextMenu.Shortcut>⌫</ContextMenu.Shortcut>
			</ContextMenu.Item>
			<ContextMenu.Item onSelect={actions.addRow}>Add row</ContextMenu.Item>
			<ContextMenu.Item disabled={!canUndo} onSelect={actions.undo}>
				Undo
				<ContextMenu.Shortcut>{shortcut('Z')}</ContextMenu.Shortcut>
			</ContextMenu.Item>
			<ContextMenu.Separator />
		{/if}
		<ContextMenu.Item disabled={!target.selected} onSelect={actions.copyValue}>
			Copy value
			<ContextMenu.Shortcut>{shortcut('C')}</ContextMenu.Shortcut>
		</ContextMenu.Item>
		<ContextMenu.Item disabled={!target.selected} onSelect={actions.copyRow}>
			Copy row
			<ContextMenu.Shortcut>{shortcut('C', true)}</ContextMenu.Shortcut>
		</ContextMenu.Item>
		<ContextMenu.Item disabled={!target.selected} onSelect={actions.openValue}>
			{editable ? 'Edit value…' : 'View value'}
		</ContextMenu.Item>
		<ContextMenu.Separator />
		<ContextMenu.Item disabled={!target.selected} onSelect={actions.copyColumnName}>
			Copy column name
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={actions.copyAll}>Copy all as TSV</ContextMenu.Item>
	{/if}
</ContextMenu.Content>
