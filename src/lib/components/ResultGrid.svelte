<script lang="ts">
	import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { toast } from 'svelte-sonner';
	import { onDestroy, untrack } from 'svelte';
	import { api, errorMessage, type SourceInfo } from '#lib/api.js';
	import {
		cellText,
		columnOffsets,
		columnWidths,
		isNumeric,
		cycleSort,
		fromTsv,
		MIN_COL_PX,
		pastedValue,
		rowAt,
		sortOrder,
		toTsv,
		visibleColumns,
		visibleRange,
		type Cell,
		type Sort
	} from '#lib/grid.js';
	import { plural } from '#lib/format.js';
	import { DEFAULT, GridEdits, buildChanges, type NewCell } from '#lib/state/edits.svelte.js';
	import ValueDialog, { type Viewing } from './ValueDialog.svelte';
	import RowFormPanel from './RowFormPanel.svelte';
	import type { RowField } from '#lib/row-form.js';
	import PanelsTopLeftIcon from '@lucide/svelte/icons/panels-top-left';
	import { highlightJson, isJsonType, type Token } from '#lib/highlight.js';
	import { dragPointer } from '#lib/drag.js';
	import {
		allDeleted,
		editChanged,
		gridItems,
		inSpan,
		openEdit,
		pastePlan,
		pickedSpan,
		planPickedDelete,
		spanItems,
		type CellEdit,
		type GridItem,
		type RowPick
	} from '#lib/grid-selection.js';
	import { joinStatements } from '#lib/review.svelte.js';
	import ReviewSqlDialog from './ReviewSqlDialog.svelte';
	import GridContextMenu, {
		type GridMenuActions,
		type GridMenuTarget
	} from './grid/GridContextMenu.svelte';
	import GridFooter from './grid/GridFooter.svelte';

	interface Props {
		columns: string[];
		rows: Cell[][];
		/** Offset added to row numbers (paging). */
		rowOffset?: number;
		/** Set for server-side sorting; without it, headers sort locally. */
		sort?: Sort | null;
		onSort?: (column: string) => void;
		primaryKeys?: string[];
		columnTypes?: Record<string, string>;
		/** Where the rows come from; null when they cannot be edited. */
		source?: SourceInfo | null;
		/** Where edits are saved; without it the grid is read-only. */
		editTarget?: { connectionId: string; database: string; readOnly: boolean } | null;
		/** Called after edits were saved, to reload the rows. */
		onSaved?: () => void;
		/** Reports the number of unsaved edits whenever it changes. */
		onDirtyChange?: (count: number) => void;
	}

	let {
		columns,
		rows,
		rowOffset = 0,
		sort = null,
		onSort,
		primaryKeys = [],
		columnTypes = {},
		source = null,
		editTarget = null,
		onSaved,
		onDirtyChange
	}: Props = $props();

	const ROW_PX = 28;
	const HEADER_PX = 32;

	type Item = GridItem;

	let scroller = $state<HTMLDivElement | null>(null);
	let scrollTop = $state(0);
	let scrollLeft = $state(0);
	let viewportHeight = $state(400);
	let viewportWidth = $state(1200);
	let widths = $state<number[]>([]);
	let selected = $state<{ row: number; col: number } | null>(null);
	/** Whole rows picked from the row numbers; `selected` stays on the moving end. */
	let rowPick = $state<RowPick | null>(null);
	let localSort = $state<Sort | null>(null);
	let viewing = $state<Viewing | null>(null);
	/** The row panel follows the selected row while open. */
	let formOpen = $state(false);
	let editing = $state<CellEdit | null>(null);
	let saving = $state(false);
	let preview = $state<string[] | null>(null);
	const edits = new GridEdits();
	let shownColumns = '';

	// New rows mean a new result or a reload after saving. Only a different
	// set of columns is a different result: then reset the view as well.
	$effect(() => {
		const key = columns.join('\u0000');
		const loaded = rows;
		// Only columns and rows trigger this; resizing a column must not wipe edits.
		untrack(() => {
			const sameShape = key === shownColumns;
			shownColumns = key;
			if (!sameShape || widths.length !== columns.length) {
				widths = columnWidths(
					columns,
					loaded,
					columns.map((name) => ({
						type: columnTypes[name],
						primaryKey: primaryKeys.includes(name)
					}))
				);
			}
			editing = null;
			edits.reset();
			rowPick = null;
			if (!sameShape) {
				selected = null;
				localSort = null;
				if (scroller) scroller.scrollTop = 0;
			}
		});
	});

	$effect(() => onDirtyChange?.(edits.count));
	onDestroy(() => onDirtyChange?.(0));

	/** Ends a row-pick or column-resize drag still running when the grid goes away. */
	let stopDrag: (() => void) | null = null;
	onDestroy(() => stopDrag?.());

	const editable = $derived(source !== null && editTarget !== null && !editTarget.readOnly);
	const activeSort = $derived(onSort ? sort : localSort);

	const order = $derived.by(() => {
		if (onSort || !localSort) return rows.map((_, i) => i);
		return sortOrder(rows, columns.indexOf(localSort.column), localSort.dir);
	});

	const items = $derived(gridItems(order, edits.inserted.length));

	const picked = $derived(pickedSpan(rowPick, items.length));
	const pickedCount = $derived(picked ? picked.end - picked.start + 1 : 0);
	const pickedAllDeleted = $derived(allDeleted(spanItems(items, picked), edits.deleted));

	function isPicked(index: number): boolean {
		return inSpan(picked, index);
	}

	function pickedItems(): Item[] {
		return spanItems(items, picked);
	}

	const numberWidth = $derived(Math.max(40, String(rowOffset + rows.length).length * 8 + 20));
	const offsets = $derived(columnOffsets(widths));
	const totalWidth = $derived(numberWidth + offsets[offsets.length - 1]);
	// Wide tables would otherwise render every column of every visible row.
	const cols = $derived(visibleColumns(offsets, scrollLeft, viewportWidth - numberWidth));
	const shownCols = $derived(
		Array.from({ length: cols.end - cols.start }, (_, i) => cols.start + i)
	);
	const range = $derived(visibleRange(scrollTop, viewportHeight - HEADER_PX, ROW_PX, items.length));
	const visible = $derived(
		items.slice(range.start, range.end).map((item, i) => ({ item, index: range.start + i }))
	);

	const jsonColumns = $derived(columns.map((name) => isJsonType(columnTypes[name])));
	// Cells re-render on every scroll step; parsing the same JSON again each time is wasted.
	// A plain cache, nothing renders from it directly, so it must not be reactive.
	// eslint-disable-next-line svelte/prefer-svelte-reactivity
	const jsonTokens = new Map<string, Token[]>();
	function jsonCell(text: string): Token[] {
		let tokens = jsonTokens.get(text);
		if (!tokens) {
			if (jsonTokens.size > 5000) jsonTokens.clear();
			tokens = highlightJson(text);
			jsonTokens.set(text, tokens);
		}
		return tokens;
	}

	function cellValue(item: Item, col: number): NewCell {
		return item.kind === 'row'
			? edits.value(item.orig, col, rows[item.orig][col])
			: edits.inserted[item.index][col];
	}

	function rowValues(item: Item): NewCell[] {
		return columns.map((_, c) => cellValue(item, c));
	}

	function canEdit(item: Item, col: number): boolean {
		if (!editable || saving || source?.columns[col] == null) return false;
		return item.kind === 'new' || !edits.deleted[item.orig];
	}

	/** `DEFAULT` only applies to new rows; a loaded row gets NULL instead. */
	function setValue(item: Item, col: number, value: NewCell) {
		if (item.kind === 'row') edits.setCell(item.orig, col, value ?? null, rows[item.orig][col]);
		else edits.setNewCell(item.index, col, value);
	}

	function startEdit(row: number, col: number, initial?: string) {
		const item = items[row];
		if (!item || !canEdit(item, col)) return;
		const current = cellValue(item, col);
		// Multi-line values are easier in the dialog than in a one-line input.
		if (initial === undefined && current?.includes('\n')) {
			openValue(row, col);
			return;
		}
		editing = openEdit(row, col, current, initial);
	}

	function commitEdit() {
		if (!editing) return;
		const item = items[editing.row];
		if (item && editChanged(editing)) setValue(item, editing.col, editing.draft);
		editing = null;
		scroller?.focus();
	}

	function cancelEdit() {
		editing = null;
		scroller?.focus();
	}

	function openValue(row: number, col: number) {
		const item = items[row];
		if (!item) return;
		viewing = {
			column: columns[col],
			value: cellValue(item, col) ?? null,
			type: columnTypes[columns[col]],
			save: canEdit(item, col) ? (value) => setValue(item, col, value) : undefined
		};
	}

	function openRowForm(row: number) {
		if (!items[row] || saving) return;
		commitEdit();
		selected = { row, col: selected?.col ?? 0 };
		formOpen = true;
		scrollToRow(row);
	}

	function closeRowForm() {
		formOpen = false;
		scroller?.focus();
	}

	const formRow = $derived(formOpen && selected && items[selected.row] ? selected.row : null);
	const formItem = $derived(formRow === null ? undefined : items[formRow]);
	const formFields = $derived<RowField[]>(
		formItem
			? columns.map((name, col) => ({
					name,
					type: columnTypes[name] ?? '',
					value: cellValue(formItem, col),
					editable: canEdit(formItem, col),
					primaryKey: primaryKeys.includes(name) || Boolean(source?.key.includes(col)),
					changed: formItem.kind === 'row' && edits.isEdited(formItem.orig, col)
				}))
			: []
	);

	function commitField(row: number, col: number, value: NewCell) {
		const item = items[row];
		if (!item || !canEdit(item, col)) return;
		if (item.kind === 'new') edits.setNewCell(item.index, col, value);
		else if (value !== DEFAULT) setValue(item, col, value);
	}

	function navigateForm(delta: number) {
		if (!selected) return;
		const next = selected.row + delta;
		if (next < 0 || next >= items.length) return;
		if (rowPick) pickRows(next, next);
		else selected = { row: next, col: selected.col };
		scrollToRow(next);
	}

	function toggleDelete(row: number) {
		const item = items[row];
		if (!item || !editable) return;
		if (item.kind === 'row') edits.toggleDelete(item.orig);
		else {
			edits.removeNewRow(item.index);
			selected = null;
		}
	}

	/** Delete the picked rows, or restore them when all of them are already deleted. */
	function togglePickedDelete() {
		if (!editable || !picked) return;
		const plan = planPickedDelete(pickedItems(), edits.deleted);
		edits.batch(() => {
			for (const orig of plan.toggle) edits.toggleDelete(orig);
			for (const index of plan.remove) edits.removeNewRow(index);
		});
		if (plan.remove.length) {
			rowPick = null;
			selected = null;
		}
	}

	function pickRows(anchor: number, focus: number) {
		rowPick = { anchor, focus };
		selected = { row: focus, col: selected?.col ?? 0 };
		scrollToRow(focus);
	}

	/** Press on a row number: pick that row, shift extends, dragging picks a span. */
	function startRowPick(e: PointerEvent, index: number) {
		if (e.button !== 0 || !scroller) return;
		e.preventDefault();
		commitEdit();
		scroller.focus();
		const anchor = e.shiftKey && rowPick ? rowPick.anchor : index;
		pickRows(anchor, index);
		let lastY = e.clientY;
		stopDrag?.();
		stopDrag = dragPointer(e, {
			move: (ev) => (lastY = ev.clientY),
			frame: () => {
				if (!scroller) return;
				const rect = scroller.getBoundingClientRect();
				// Dragging past the top or bottom edge scrolls, faster the further out.
				const edge = 24;
				if (lastY < rect.top + HEADER_PX + edge) {
					scroller.scrollTop -= Math.min(40, rect.top + HEADER_PX + edge - lastY) / 2;
				} else if (lastY > rect.bottom - edge) {
					scroller.scrollTop += Math.min(40, lastY - (rect.bottom - edge)) / 2;
				}
				const focus = rowAt(lastY - rect.top + scroller.scrollTop, HEADER_PX, ROW_PX, items.length);
				if (rowPick && focus !== rowPick.focus) {
					rowPick = { anchor, focus };
					selected = { row: focus, col: selected?.col ?? 0 };
				}
			},
			end: () => (stopDrag = null)
		});
	}

	function addRow() {
		edits.addRow(columns.length);
		const row = items.length - 1;
		rowPick = null;
		selected = { row, col: 0 };
		scrollToRow(row);
	}

	function revertCell(row: number, col: number) {
		const item = items[row];
		if (item?.kind === 'row')
			edits.setCell(item.orig, col, rows[item.orig][col], rows[item.orig][col]);
	}

	/** Paste a cell or a block from a spreadsheet, starting at the selection. */
	function paste(text: string) {
		if (!selected || !editable) return;
		const plan = pastePlan(selected, fromTsv(text), columns.length, items.length);
		edits.batch(() => {
			for (let n = 0; n < plan.addRows; n++) edits.addRow(columns.length);
			for (const { row, col, value } of plan.cells) {
				const item = items[row];
				if (item && canEdit(item, col))
					setValue(item, col, pastedValue(value, item.kind === 'new'));
			}
		});
	}

	async function showPreview() {
		if (!source) return;
		try {
			preview = await api.previewChanges(
				source.schema,
				source.table,
				buildChanges(edits, rows, source)
			);
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function save() {
		if (!source || !editTarget || saving || edits.count === 0) return;
		const changes = buildChanges(edits, rows, source);
		if (!changes.length) {
			edits.reset();
			return;
		}
		saving = true;
		try {
			const n = await api.applyChanges(
				editTarget.connectionId,
				editTarget.database,
				source.schema,
				source.table,
				changes
			);
			toast.success(`Saved ${plural(n, 'change')} to ${source.table}`);
			edits.reset();
			onSaved?.();
		} catch (e) {
			toast.error(errorMessage(e), { duration: 10_000 });
		} finally {
			saving = false;
		}
	}

	function toggleSort(column: string) {
		if (onSort) {
			onSort(column);
			return;
		}
		localSort = cycleSort(localSort, column);
	}

	function startResize(event: PointerEvent, col: number) {
		event.preventDefault();
		event.stopPropagation();
		const startX = event.clientX;
		const startWidth = widths[col];
		stopDrag?.();
		stopDrag = dragPointer(event, {
			move: (e) => {
				widths[col] = Math.max(MIN_COL_PX, startWidth + e.clientX - startX);
			},
			end: () => (stopDrag = null)
		});
	}

	async function copy(text: string, what: string) {
		await navigator.clipboard.writeText(text);
		toast.success(`Copied ${what}`);
	}

	function selectedValue(): Cell {
		if (!selected || !items[selected.row]) return null;
		return cellValue(items[selected.row], selected.col) ?? null;
	}

	function scrollToRow(row: number) {
		if (!scroller) return;
		const top = row * ROW_PX;
		const bottom = top + ROW_PX + HEADER_PX;
		if (top < scroller.scrollTop) scroller.scrollTop = top;
		else if (bottom > scroller.scrollTop + viewportHeight) {
			scroller.scrollTop = bottom - viewportHeight;
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (editing) return;
		const mod = e.metaKey || e.ctrlKey;
		if (mod && e.key === 's') {
			e.preventDefault();
			void save();
			return;
		}
		if (mod && e.key.toLowerCase() === 'z') {
			e.preventDefault();
			if (e.shiftKey) edits.redo();
			else edits.undo();
			return;
		}
		if (!selected) return;
		if (picked && onPickedKey(e, mod)) return;
		if (mod && e.key === 'c') {
			e.preventDefault();
			if (e.shiftKey) void copy(toTsv(null, [rowValues(items[selected.row])]), 'row');
			else void copy(selectedValue() ?? '', 'value');
			return;
		}
		const moves: Record<string, [number, number]> = {
			ArrowUp: [-1, 0],
			ArrowDown: [1, 0],
			ArrowLeft: [0, -1],
			ArrowRight: [0, 1],
			Tab: [0, e.shiftKey ? -1 : 1]
		};
		const move = moves[e.key];
		if (move) {
			e.preventDefault();
			const row = Math.min(items.length - 1, Math.max(0, selected.row + move[0]));
			const col = Math.min(columns.length - 1, Math.max(0, selected.col + move[1]));
			selected = { row, col };
			scrollToRow(row);
		} else if (e.key === 'Enter' && e.shiftKey) {
			e.preventDefault();
			openRowForm(selected.row);
		} else if (e.key === 'Enter' || e.key === 'F2') {
			e.preventDefault();
			if (canEdit(items[selected.row], selected.col)) startEdit(selected.row, selected.col);
			else openValue(selected.row, selected.col);
		} else if ((e.key === 'Delete' || e.key === 'Backspace') && editable) {
			e.preventDefault();
			toggleDelete(selected.row);
		} else if (e.key.length === 1 && !mod && !e.altKey) {
			// Typing on a cell starts editing it, like a spreadsheet.
			if (canEdit(items[selected.row], selected.col)) {
				e.preventDefault();
				startEdit(selected.row, selected.col, e.key);
			}
		}
	}

	/** Keys while whole rows are picked; returns false to fall through to cell keys. */
	function onPickedKey(e: KeyboardEvent, mod: boolean): boolean {
		if (!rowPick || !picked) return false;
		if (mod && e.key === 'c') {
			e.preventDefault();
			void copyPicked();
		} else if (e.key === 'Escape') {
			rowPick = null;
		} else if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && !mod) {
			e.preventDefault();
			const focus = Math.min(
				items.length - 1,
				Math.max(0, rowPick.focus + (e.key === 'ArrowUp' ? -1 : 1))
			);
			pickRows(e.shiftKey ? rowPick.anchor : focus, focus);
		} else if ((e.key === 'Delete' || e.key === 'Backspace') && editable) {
			e.preventDefault();
			togglePickedDelete();
		} else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Tab') {
			// Moving sideways leaves row picking and continues on the cell.
			rowPick = null;
			return false;
		} else if (e.key === 'Enter' && e.shiftKey) {
			return false;
		} else if (e.key.length === 1 || e.key === 'Enter' || e.key === 'F2') {
			// Typing does not edit when whole rows are picked.
			e.preventDefault();
		} else return false;
		return true;
	}

	function copyPicked(withHeader = false) {
		const chosen = pickedItems();
		return copy(
			toTsv(withHeader ? columns : null, chosen.map(rowValues)),
			chosen.length === 1 ? 'row' : `${chosen.length} rows`
		);
	}

	function cellFromEvent(e: MouseEvent) {
		const target = e.target as HTMLElement;
		const number = target.closest<HTMLElement>('[data-row-number]');
		if (number) {
			// Right-clicking a row number outside the picked rows picks that row instead.
			const index = Number(number.dataset.rowNumber);
			if (!isPicked(index)) pickRows(index, index);
			return;
		}
		const el = target.closest<HTMLElement>('[data-row]');
		if (!el) return;
		rowPick = null;
		selected = { row: Number(el.dataset.row), col: Number(el.dataset.col) };
	}

	function gutterClass(item: Item, index: number): string {
		if (isPicked(index)) return 'bg-signal-soft text-signal-text font-medium';
		if (item.kind === 'new') return 'bg-ok-soft text-ok';
		if (edits.deleted[item.orig]) return 'bg-bad-soft text-bad';
		return 'bg-background text-muted-foreground/70 group-hover/row:bg-hover-row';
	}

	function focusInput(node: HTMLInputElement) {
		node.focus();
		const end = node.value.length;
		node.setSelectionRange(end, end);
	}

	function rowClass(item: Item, index: number): string {
		if (isPicked(index)) {
			const deleted = item.kind === 'row' && edits.deleted[item.orig];
			return `bg-signal-soft ${deleted ? 'text-muted-foreground line-through' : ''}`;
		}
		if (item.kind === 'new') return 'bg-ok-soft';
		if (edits.deleted[item.orig]) return 'bg-bad-soft text-muted-foreground line-through';
		// The selected cell's row is tinted so the eye can follow it across wide tables.
		return selected?.row === index ? 'bg-hover-row' : 'hover:bg-hover-row';
	}

	const selectedItem = $derived(selected ? items[selected.row] : undefined);
	const selectedDeleted = $derived(
		selectedItem?.kind === 'row' && Boolean(edits.deleted[selectedItem.orig])
	);

	const menuTarget = $derived<GridMenuTarget>(
		picked
			? { kind: 'rows', count: pickedCount, allDeleted: pickedAllDeleted }
			: {
					kind: 'cell',
					selected: selected !== null,
					canEdit: !!selected && !!selectedItem && canEdit(selectedItem, selected.col),
					canRevert:
						!!selected &&
						selectedItem?.kind === 'row' &&
						edits.isEdited(selectedItem.orig, selected.col),
					rowDeleted: selectedDeleted
				}
	);

	const menuActions: GridMenuActions = {
		openRowForm: () => {
			if (picked) openRowForm(picked.start);
			else if (selected) openRowForm(selected.row);
		},
		copyRows: (withHeader) => void copyPicked(withHeader),
		toggleDelete: () => {
			if (picked) togglePickedDelete();
			else if (selected) toggleDelete(selected.row);
		},
		addRow,
		editCell: () => selected && startEdit(selected.row, selected.col),
		setNull: () => selected && selectedItem && setValue(selectedItem, selected.col, null),
		revertCell: () => selected && revertCell(selected.row, selected.col),
		undo: () => edits.undo(),
		copyValue: () => void copy(selectedValue() ?? '', 'value'),
		copyRow: () => selected && void copy(toTsv(null, [rowValues(items[selected.row])]), 'row'),
		openValue: () => selected && openValue(selected.row, selected.col),
		copyColumnName: () => selected && void copy(columns[selected.col], 'column name'),
		copyAll: () => void copy(toTsv(columns, items.map(rowValues)), `${items.length} rows`)
	};
</script>

<div class="relative flex h-full min-h-0">
	<div class="flex min-w-0 flex-1 flex-col">
		<div class="flex h-8 shrink-0 items-center gap-3 border-b px-2 text-xs">
			<Button
				size="xs"
				variant="ghost"
				disabled={!items.length || saving}
				aria-pressed={formOpen}
				onclick={() => (formOpen ? closeRowForm() : openRowForm(selected?.row ?? 0))}
			>
				<PanelsTopLeftIcon /> Row form
			</Button>
			<span class="text-muted-foreground"
				>{formOpen ? 'Follows the selected row' : 'All fields of a row'} · ⇧↵</span
			>
		</div>
		<ContextMenu.Root>
			<ContextMenu.Trigger>
				{#snippet child({ props })}
					<div
						{...props}
						bind:this={scroller}
						bind:clientHeight={viewportHeight}
						bind:clientWidth={viewportWidth}
						onscroll={(e) => {
							scrollTop = e.currentTarget.scrollTop;
							scrollLeft = e.currentTarget.scrollLeft;
						}}
						oncontextmenu={(e) => {
							cellFromEvent(e);
							(props.oncontextmenu as ((e: MouseEvent) => void) | undefined)?.(e);
						}}
						onkeydown={onKeydown}
						onpaste={(e) => {
							const text = e.clipboardData?.getData('text/plain');
							if (text !== undefined && editable && selected) {
								e.preventDefault();
								paste(text);
							}
						}}
						tabindex="0"
						role="grid"
						aria-rowcount={items.length}
						class="relative min-h-0 min-w-0 flex-1 overflow-auto bg-background text-[13px] tabular-nums select-none"
					>
						<div
							class="relative"
							style:width="{totalWidth}px"
							style:height="{HEADER_PX + items.length * ROW_PX}px"
						>
							<div
								class="sticky top-0 z-20 flex border-b bg-background"
								style:height="{HEADER_PX}px"
							>
								<div
									class="sticky left-0 z-10 shrink-0 bg-background"
									style:width="{numberWidth}px"
								></div>
								<div class="shrink-0" style:width="{offsets[cols.start]}px"></div>
								{#each shownCols as c (c)}
									{@const column = columns[c]}
									<div
										class="group after:bg-rule relative flex shrink-0 items-center after:absolute after:inset-y-2 after:right-0 after:w-px"
										style:width="{widths[c]}px"
									>
										<button
											type="button"
											class="flex h-full min-w-0 flex-1 items-center gap-1.5 px-2.5 text-left text-xs font-medium text-ink-2 transition-[background-color,color] duration-(--dur-micro) hover:bg-hover hover:text-foreground"
											title={columnTypes[column] ? `${column}: ${columnTypes[column]}` : column}
											onclick={() => toggleSort(column)}
										>
											{#if primaryKeys.includes(column)}
												<KeyRoundIcon
													class="size-3 shrink-0 text-muted-foreground"
													aria-label="Primary key"
												/>
											{/if}
											<span class="truncate">{column}</span>
											{#if columnTypes[column]}
												<span class="truncate text-[11px] font-normal text-muted-foreground/80">
													{columnTypes[column]}
												</span>
											{/if}
											{#if activeSort?.column === column}
												{#if activeSort.dir === 'asc'}
													<ArrowUpIcon
														class="ml-auto size-3.5 shrink-0 text-signal-text"
														aria-label="Ascending"
													/>
												{:else}
													<ArrowDownIcon
														class="ml-auto size-3.5 shrink-0 text-signal-text"
														aria-label="Descending"
													/>
												{/if}
											{/if}
										</button>
										<!-- svelte-ignore a11y_no_static_element_interactions -->
										<div
											class="absolute top-0 -right-1 z-10 h-full w-2 cursor-col-resize after:absolute after:inset-y-1.5 after:left-1/2 after:w-0.5 after:-translate-x-1/2 after:rounded-full after:bg-signal after:opacity-0 after:transition-opacity hover:after:opacity-100"
											onpointerdown={(e) => startResize(e, c)}
										></div>
									</div>
								{/each}
							</div>

							{#each visible as { item, index } (index)}
								<div
									class="group/row border-rule/50 absolute left-0 flex border-b {rowClass(
										item,
										index
									)}"
									style:top="{HEADER_PX + index * ROW_PX}px"
									style:height="{ROW_PX}px"
									style:width="{totalWidth}px"
								>
									<!-- svelte-ignore a11y_no_static_element_interactions -->
									<div
										data-row-number={index}
										class="sticky left-0 z-10 flex shrink-0 cursor-default items-center justify-end pr-3 pl-2 text-[11px] no-underline {gutterClass(
											item,
											index
										)}"
										style:width="{numberWidth}px"
										onpointerdown={(e) => startRowPick(e, index)}
									>
										{item.kind === 'new' ? '+' : rowOffset + index + 1}
									</div>
									<div class="shrink-0" style:width="{offsets[cols.start]}px"></div>
									{#each shownCols as c (c)}
										{@const value = cellValue(item, c)}
										{@const isSelected = !rowPick && selected?.row === index && selected?.col === c}
										{@const changed = item.kind === 'row' && edits.isEdited(item.orig, c)}
										<!-- svelte-ignore a11y_click_events_have_key_events -->
										<div
											role="gridcell"
											tabindex="-1"
											data-row={index}
											data-col={c}
											class="relative flex shrink-0 items-center overflow-hidden px-2.5 whitespace-pre {isSelected
												? 'bg-signal-soft/60 ring-2 ring-signal ring-inset'
												: changed
													? 'bg-signal-soft shadow-[inset_2px_0_0_var(--color-accent)]'
													: ''}"
											class:justify-end={isNumeric(value ?? null)}
											style:width="{widths[c]}px"
											title={changed && item.kind === 'row'
												? `was: ${rows[item.orig][c] ?? 'NULL'}`
												: undefined}
											onclick={cellFromEvent}
											ondblclick={() => {
												if (canEdit(item, c)) startEdit(index, c);
												else openValue(index, c);
											}}
										>
											{#if editing?.row === index && editing.col === c}
												<input
													use:focusInput
													bind:value={editing.draft}
													aria-label="Cell value"
													class="absolute inset-0 bg-background px-2.5 text-[13px] outline-2 -outline-offset-2 outline-signal"
													onkeydown={(e) => {
														e.stopPropagation();
														if (e.key === 'Enter') commitEdit();
														else if (e.key === 'Escape') cancelEdit();
													}}
													onblur={commitEdit}
												/>
											{:else if value === DEFAULT}
												<span class="text-[11px] tracking-wide text-muted-foreground/70 italic"
													>DEFAULT</span
												>
											{:else if value === null}
												<span class="text-[11px] tracking-wide text-muted-foreground/70 italic"
													>NULL</span
												>
											{:else if jsonColumns[c]}
												<span class="json-tokens measured truncate text-xs"
													>{#each jsonCell(cellText(value)) as token, t (t)}<span
															class={token.classes}>{token.text}</span
														>{/each}</span
												>
											{:else}
												<span class="truncate">{cellText(value)}</span>
											{/if}
										</div>
									{/each}
								</div>
							{/each}
						</div>
					</div>
				{/snippet}
			</ContextMenu.Trigger>
			<GridContextMenu
				target={menuTarget}
				actions={menuActions}
				{editable}
				{saving}
				canUndo={edits.canUndo}
			/>
		</ContextMenu.Root>

		<GridFooter
			{edits}
			{editable}
			{source}
			{editTarget}
			{saving}
			onAddRow={addRow}
			onPreview={showPreview}
			onSave={save}
		/>
	</div>
	{#if formItem && formRow !== null}
		<RowFormPanel
			rowKey={formItem.kind === 'row' ? `row:${formItem.orig}` : `new:${formItem.index}`}
			number={rowOffset + formRow + 1}
			position={formRow}
			total={items.length}
			isNew={formItem.kind === 'new'}
			deleted={formItem.kind === 'row' && Boolean(edits.deleted[formItem.orig])}
			fields={formFields}
			{source}
			{editTarget}
			onCommit={(col, value) => formRow !== null && commitField(formRow, col, value)}
			onNavigate={navigateForm}
			onClose={closeRowForm}
			onSave={() => void save()}
		/>
	{/if}
</div>

<ValueDialog bind:viewing />

<ReviewSqlDialog
	open={preview !== null}
	title="Pending SQL"
	sql={joinStatements(preview ?? [])}
	cancelLabel="Close"
	confirmLabel="Save"
	confirmDisabled={saving}
	sqlClass="max-h-[55vh] overflow-auto rounded-lg border bg-background p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap select-text"
	onClose={() => (preview = null)}
	onConfirm={async () => {
		preview = null;
		await save();
	}}
>
	{#snippet description()}
		Runs in one transaction. If any UPDATE or DELETE does not hit exactly one row, nothing is saved.
	{/snippet}
</ReviewSqlDialog>
