<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import {
		clampPanelWidth,
		fieldKind,
		formattedJson,
		isFieldEdit,
		ROW_PANEL_DEFAULT_PX,
		type RowField
	} from '#lib/row-form.js';
	import type { NewCell } from '#lib/state/edits.svelte.js';
	import { toast } from 'svelte-sonner';
	import ChevronUpIcon from '@lucide/svelte/icons/chevron-up';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import BracesIcon from '@lucide/svelte/icons/braces';
	import XIcon from '@lucide/svelte/icons/x';
	import MaximizeIcon from '@lucide/svelte/icons/maximize-2';
	import MinimizeIcon from '@lucide/svelte/icons/minimize-2';
	import JsonEditor from './JsonEditor.svelte';

	interface Props {
		/** Identifies the shown row; switching it resets per-row form state. */
		rowKey: string;
		number: number;
		position: number;
		total: number;
		isNew: boolean;
		deleted: boolean;
		fields: RowField[];
		onCommit: (column: number, value: NewCell) => void;
		onNavigate: (delta: number) => void;
		onClose: () => void;
		onSave?: () => void;
	}

	let {
		rowKey,
		number,
		position,
		total,
		isNew,
		deleted,
		fields,
		onCommit,
		onNavigate,
		onClose,
		onSave
	}: Props = $props();

	const WIDTH_KEY = 'guckpg.rowPanelWidth';
	const MAXIMIZED_KEY = 'guckpg.rowPanelMaximized';

	let panel: HTMLElement;
	let width = $state(ROW_PANEL_DEFAULT_PX);
	/** Covers the whole result area instead of sitting beside the grid. */
	let maximized = $state(localStorage.getItem(MAXIMIZED_KEY) === 'true');

	function toggleMaximized() {
		maximized = !maximized;
		localStorage.setItem(MAXIMIZED_KEY, String(maximized));
	}
	/** Text typed before switching a field to NULL or DEFAULT, to bring it back. */
	let lastText = $state<Record<number, string>>({});

	$effect(() => {
		void rowKey;
		lastText = {};
	});

	onMount(() => {
		width = clampPanelWidth(
			Number(localStorage.getItem(WIDTH_KEY) ?? ROW_PANEL_DEFAULT_PX),
			panel.parentElement?.clientWidth ?? 1200
		);
	});

	function startResize(event: PointerEvent) {
		event.preventDefault();
		const startX = event.clientX;
		const startWidth = width;
		const available = panel.parentElement?.clientWidth ?? 1200;
		const move = (e: PointerEvent) => {
			width = clampPanelWidth(startWidth + startX - e.clientX, available);
		};
		const up = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			localStorage.setItem(WIDTH_KEY, String(width));
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
	}

	function commit(index: number, value: NewCell) {
		const field = fields[index];
		if (field && isFieldEdit(field, value, isNew)) onCommit(index, value);
	}

	/** Toggle NULL or DEFAULT; toggling off brings back the text from before. */
	function toggleMode(index: number, mode: 'null' | 'default') {
		const current = fields[index].value;
		if (typeof current === 'string') lastText[index] = current;
		const active = mode === 'null' ? current === null : current === undefined;
		commit(index, active ? (lastText[index] ?? '') : mode === 'null' ? null : undefined);
	}

	function format(index: number) {
		const value = fields[index].value;
		const result = typeof value === 'string' ? formattedJson(value) : null;
		if (result === null) toast.error('This value is not valid JSON');
		else commit(index, result);
	}

	/** Blurring commits the focused field before the panel moves on or closes. */
	function leaveField() {
		const active = document.activeElement;
		if (active instanceof HTMLElement && panel.contains(active)) active.blur();
	}

	function onKeydown(event: KeyboardEvent) {
		const mod = event.metaKey || event.ctrlKey;
		if (event.key === 'Escape') {
			event.preventDefault();
			leaveField();
			// Esc steps back one level: a maximized panel shrinks first, then closes.
			if (maximized) toggleMaximized();
			else onClose();
		} else if (mod && event.key.toLowerCase() === 's') {
			event.preventDefault();
			leaveField();
			onSave?.();
		} else if (event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
			event.preventDefault();
			leaveField();
			onNavigate(event.key === 'ArrowUp' ? -1 : 1);
		}
	}

	const editableCount = $derived(fields.filter((field) => field.editable).length);
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside
	bind:this={panel}
	class="flex flex-col bg-chrome {maximized
		? 'absolute inset-0 z-30'
		: 'relative shrink-0 border-l'}"
	style:width={maximized ? undefined : `${width}px`}
	aria-label="Row {number}"
	onkeydown={onKeydown}
>
	{#if !maximized}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize after:absolute after:inset-y-0 after:left-1/2 after:w-px after:-translate-x-1/2 after:bg-signal after:opacity-0 after:transition-opacity hover:after:opacity-100"
			title="Drag to resize"
			onpointerdown={startResize}
		></div>
	{/if}

	<!-- Same height as the grid toolbar so both top rules line up. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<header
		class="flex h-8 shrink-0 items-center gap-2 border-b pr-1 pl-4"
		ondblclick={(event) => {
			if (!(event.target as HTMLElement).closest('button')) toggleMaximized();
		}}
	>
		<h2 class="truncate text-xs font-semibold">
			{isNew ? 'New row' : `Row ${number}`}
		</h2>
		{#if isNew}<span class="chip chip-ok">new</span>{/if}
		{#if deleted}<span class="chip border-bad/45 bg-bad-soft text-bad">deleted</span>{/if}
		<div class="ml-auto flex items-center">
			<Button
				variant="ghost"
				size="icon-xs"
				disabled={position === 0}
				onclick={() => onNavigate(-1)}
				title="Previous row (⌥↑)"
				aria-label="Previous row"><ChevronUpIcon /></Button
			>
			<span class="measured min-w-14 text-center text-[11px] text-muted-foreground"
				>{position + 1} / {total}</span
			>
			<Button
				variant="ghost"
				size="icon-xs"
				disabled={position + 1 >= total}
				onclick={() => onNavigate(1)}
				title="Next row (⌥↓)"
				aria-label="Next row"><ChevronDownIcon /></Button
			>
			<Button
				variant="ghost"
				size="icon-xs"
				class="ml-1"
				onclick={toggleMaximized}
				title={maximized ? 'Back to the side (Esc)' : 'Fill the result area'}
				aria-label={maximized ? 'Restore row panel' : 'Maximize row panel'}
				aria-pressed={maximized}
				>{#if maximized}<MinimizeIcon />{:else}<MaximizeIcon />{/if}</Button
			>
			<Button
				variant="ghost"
				size="icon-xs"
				onclick={onClose}
				title="Close (Esc)"
				aria-label="Close row panel"><XIcon /></Button
			>
		</div>
	</header>

	{#if deleted || !editableCount}
		<p class="border-b px-4 py-1.5 text-[11px] text-muted-foreground">
			{deleted ? 'Marked for deletion. Restore the row in the grid to edit it.' : 'Read-only row'}
		</p>
	{/if}

	<div class="min-h-0 flex-1 overflow-auto py-1.5">
		<!-- Maximized, the fields keep a readable width instead of stretching across the screen. -->
		<div class={maximized ? 'mx-auto w-full max-w-3xl py-4' : ''}>
			{#each fields as field, index (index)}
				{@const value = field.value}
				{@const kind = fieldKind(field.type, typeof value === 'string' ? value : '')}
				<div class="relative px-4 py-2 {field.changed ? 'bg-signal-soft/40' : ''}">
					{#if field.changed}
						<span class="absolute inset-y-2 left-0 w-0.5 rounded-full bg-signal"></span>
					{/if}
					<div class="mb-1.5 flex min-h-5 items-center gap-1.5">
						{#if field.primaryKey}<KeyRoundIcon
								class="size-3 shrink-0 text-muted-foreground"
								aria-label="Primary key"
							/>{/if}
						<label
							for={`row-field-${index}`}
							class="truncate text-[13px] font-medium"
							title={field.name}>{field.name}</label
						>
						<span class="truncate text-[11px] text-muted-foreground/80"
							>{field.type || 'result column'}{!field.editable && !deleted
								? ' · read-only'
								: ''}</span
						>
						{#if field.editable}
							<span class="ml-auto flex shrink-0 items-center gap-1">
								{#if kind === 'json' && typeof value === 'string'}
									<Button
										size="icon-xs"
										variant="ghost"
										title="Format JSON"
										aria-label={`Format ${field.name} as JSON`}
										onclick={() => format(index)}><BracesIcon /></Button
									>
								{/if}
								<button
									type="button"
									class="chip h-5 transition-colors {value === null
										? 'chip-signal'
										: 'border-transparent text-muted-foreground/70 hover:border-rule-2 hover:text-foreground'}"
									aria-pressed={value === null}
									title={value === null ? 'Back to a value' : 'Set NULL'}
									onclick={() => toggleMode(index, 'null')}>NULL</button
								>
								{#if isNew}
									<button
										type="button"
										class="chip h-5 transition-colors {value === undefined
											? 'chip-signal'
											: 'border-transparent text-muted-foreground/70 hover:border-rule-2 hover:text-foreground'}"
										aria-pressed={value === undefined}
										title={value === undefined ? 'Back to a value' : 'Use the database default'}
										onclick={() => toggleMode(index, 'default')}>DEFAULT</button
									>
								{/if}
							</span>
						{/if}
					</div>

					{#if typeof value !== 'string'}
						{#if field.editable}
							<button
								type="button"
								id={`row-field-${index}`}
								class="hover:border-field-edge flex h-8 w-full items-center rounded-lg border border-dashed border-rule-2 px-3 text-left text-xs text-muted-foreground/80 italic transition-colors hover:text-muted-foreground"
								title="Click to enter a value"
								onclick={() => commit(index, lastText[index] ?? '')}
							>
								{value === null ? 'NULL' : 'DEFAULT · database default'}
							</button>
						{:else}
							<div class="px-0.5 text-xs text-muted-foreground/70 italic">
								{value === null ? 'NULL' : 'DEFAULT'}
							</div>
						{/if}
					{:else if kind === 'json'}
						<JsonEditor
							id={`row-field-${index}`}
							label={field.name}
							value={formattedJson(value) ?? value}
							readonly={!field.editable}
							onblur={(next) => commit(index, next)}
						/>
					{:else if !field.editable}
						<div
							id={`row-field-${index}`}
							class="px-0.5 text-[13px] break-words whitespace-pre-wrap tabular-nums select-text"
						>
							{value}
						</div>
					{:else if kind === 'bool'}
						<div
							class="inline-flex h-8 items-center rounded-lg border border-input bg-background p-0.5"
							role="radiogroup"
							aria-label={field.name}
						>
							{#each ['true', 'false'] as option (option)}
								<button
									type="button"
									role="radio"
									aria-checked={value === option}
									class="h-full rounded-md px-3 text-xs font-medium transition-colors {value ===
									option
										? 'bg-hover text-foreground'
										: 'text-muted-foreground hover:text-foreground'}"
									onclick={() => commit(index, option)}>{option}</button
								>
							{/each}
						</div>
					{:else if kind === 'multiline'}
						<textarea
							id={`row-field-${index}`}
							class="field-sizing-content max-h-80 min-h-16 w-full resize-y rounded-lg border border-input bg-background px-3 py-1.5 text-[13px] leading-relaxed"
							spellcheck={false}
							{value}
							onchange={(event) => commit(index, event.currentTarget.value)}></textarea>
					{:else}
						<input
							id={`row-field-${index}`}
							class="h-8 w-full rounded-lg border border-input bg-background px-3 text-[13px] tabular-nums"
							spellcheck={false}
							autocomplete="off"
							{value}
							onchange={(event) => commit(index, event.currentTarget.value)}
							onkeydown={(event) => {
								if (event.key === 'Enter') commit(index, event.currentTarget.value);
							}}
						/>
					{/if}
				</div>
			{/each}
		</div>
	</div>
</aside>
