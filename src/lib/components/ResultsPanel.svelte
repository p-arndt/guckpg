<script lang="ts">
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import { errorMessage, type AppError, type QueryOutput } from '#lib/api.js';
	import { formatCount, formatDuration } from '#lib/format.js';
	import type { ComponentProps } from 'svelte';
	import ResultGrid from './ResultGrid.svelte';

	interface Props {
		running: boolean;
		output: QueryOutput | null;
		error: AppError | null;
		editTarget?: ComponentProps<typeof ResultGrid>['editTarget'];
		onSaved?: () => void;
		onDirtyChange?: (count: number) => void;
	}

	let { running, output, error, editTarget = null, onSaved, onDirtyChange }: Props = $props();

	let picked = $state<number | null>(null);

	// The last result with rows is what people want to see after a script.
	const defaultIndex = $derived.by(() => {
		const results = output?.results ?? [];
		for (let i = results.length - 1; i >= 0; i--) if (results[i].columns.length) return i;
		return results.length - 1;
	});

	$effect(() => {
		void output;
		picked = null;
	});

	const index = $derived(picked ?? defaultIndex);
	const current = $derived(output?.results[index]);
</script>

<div class="flex h-full min-h-0 flex-col">
	{#if output && output.results.length > 1}
		<div class="flex shrink-0 gap-1 overflow-x-auto border-b px-2 py-1">
			{#each output.results as result, i (i)}
				<button
					class="rounded px-2 py-0.5 text-xs whitespace-nowrap {i === index
						? 'bg-accent font-medium text-accent-foreground'
						: 'text-muted-foreground hover:bg-accent/60'}"
					onclick={() => (picked = i)}
				>
					{i + 1}: {result.columns.length
						? `${formatCount(result.rowCount)} rows`
						: `${formatCount(result.rowCount)} affected`}
				</button>
			{/each}
		</div>
	{/if}

	<div class="min-h-0 flex-1">
		{#if error && !running}
			<div class="m-3 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm">
				<div class="flex items-start gap-2 text-destructive">
					<CircleAlertIcon class="mt-0.5 size-4 shrink-0" />
					<div class="min-w-0 font-medium break-words whitespace-pre-wrap select-text">
						{errorMessage(error)}
					</div>
				</div>
				{#if error.kind === 'postgres'}
					<div class="mt-2 space-y-1 pl-6 text-xs text-muted-foreground select-text">
						{#if error.detail}<div>
								<span class="font-medium">Detail:</span>
								{error.detail}
							</div>{/if}
						{#if error.hint}<div><span class="font-medium">Hint:</span> {error.hint}</div>{/if}
						{#if error.code}<div class="font-mono">SQLSTATE {error.code}</div>{/if}
					</div>
				{/if}
			</div>
		{:else if current && current.columns.length}
			<ResultGrid
				columns={current.columns}
				rows={current.rows}
				source={current.source}
				{editTarget}
				{onSaved}
				{onDirtyChange}
			/>
		{:else if current}
			<div class="p-4 text-sm text-muted-foreground">
				Statement done, {formatCount(current.rowCount)} rows affected.
			</div>
		{:else if !running}
			<div class="flex h-full items-center justify-center text-sm text-muted-foreground">
				<span>
					<kbd class="rounded border px-1 font-mono text-xs">⌘ ↵</kbd> runs the statement at the
					cursor, <kbd class="rounded border px-1 font-mono text-xs">⇧ ⌘ ↵</kbd> runs everything
				</span>
			</div>
		{/if}
	</div>

	<div class="flex h-7 shrink-0 items-center gap-3 border-t px-3 text-xs text-muted-foreground">
		{#if running}
			<LoaderIcon class="size-3.5 animate-spin" />
			<span>Running…</span>
		{:else if output}
			{#if current?.columns.length}
				<span>{formatCount(current.rows.length)} rows</span>
				{#if current.truncated}
					<span class="text-amber-600 dark:text-amber-400">
						showing first {formatCount(current.rows.length)} of {formatCount(current.rowCount)}
					</span>
				{/if}
			{/if}
			<span>{formatDuration(output.elapsedMs)}</span>
		{/if}
	</div>
</div>
