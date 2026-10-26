<script lang="ts">
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import TableIcon from '@lucide/svelte/icons/table-2';
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
	{#if running || output || error}
		<div class="flex h-9 shrink-0 items-center gap-3 border-b px-3 text-xs text-muted-foreground">
			{#if output && output.results.length > 1}
				<div class="flex min-w-0 gap-1 overflow-x-auto" role="tablist" aria-label="Results">
					{#each output.results as result, i (i)}
						<button
							type="button"
							role="tab"
							aria-selected={i === index}
							class="flex h-6 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2 transition-[background-color,color] duration-(--dur-micro) {i ===
							index
								? 'bg-hover text-foreground'
								: 'hover:bg-hover/60 hover:text-foreground'}"
							onclick={() => (picked = i)}
						>
							<span class="measured text-muted-foreground">{i + 1}</span>
							<span>
								<span class="measured">{formatCount(result.rowCount)}</span>
								{result.columns.length ? 'rows' : 'affected'}
							</span>
						</button>
					{/each}
				</div>
			{:else}
				<span class="text-[13px] font-medium text-foreground">Results</span>
			{/if}
			<div class="ml-auto flex shrink-0 items-center gap-3" role="status">
				{#if running}
					<LoaderIcon class="size-3.5 animate-spin text-cool" />
					<span>Running…</span>
				{:else if error}
					<span class="text-bad">Failed</span>
				{:else if output}
					{#if current?.truncated}
						<span class="chip chip-signal">
							first <span class="measured">{formatCount(current.rows.length)}</span> of
							<span class="measured">{formatCount(current.rowCount)}</span>
						</span>
					{:else if current?.columns.length}
						<span
							><span class="measured text-cool">{formatCount(current.rows.length)}</span> rows</span
						>
					{/if}
					<span class="measured text-cool">{formatDuration(output.elapsedMs)}</span>
				{/if}
			</div>
		</div>
	{/if}

	<div class="min-h-0 flex-1">
		{#if error && !running}
			<div
				class="m-3 max-w-3xl rounded-xl border border-[color-mix(in_oklab,var(--color-bad),transparent_55%)] bg-bad-soft p-4 select-text"
				role="alert"
			>
				<div class="flex items-start gap-2.5">
					<CircleAlertIcon class="mt-0.5 size-4 shrink-0 text-bad" />
					<p class="min-w-0 font-medium break-words whitespace-pre-wrap">
						{errorMessage(error)}
					</p>
				</div>
				{#if error.kind === 'postgres' && (error.detail || error.hint || error.code)}
					<dl
						class="mt-3 grid grid-cols-[max-content_minmax(0,1fr)] gap-x-4 gap-y-1 pl-6.5 text-[13px]"
					>
						{#if error.detail}
							<dt class="text-muted-foreground">Detail</dt>
							<dd class="text-ink-2">{error.detail}</dd>
						{/if}
						{#if error.hint}
							<dt class="text-muted-foreground">Hint</dt>
							<dd class="text-ink-2">{error.hint}</dd>
						{/if}
						{#if error.code}
							<dt class="text-muted-foreground">SQLSTATE</dt>
							<dd class="measured text-ink-2">{error.code}</dd>
						{/if}
					</dl>
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
			<div class="flex flex-col gap-1 p-6">
				<p class="text-[13px] text-muted-foreground">Statement done</p>
				<p class="flex items-baseline gap-2">
					<span class="measured text-3xl leading-none tracking-[-0.03em]"
						>{formatCount(current.rowCount)}</span
					>
					<span class="text-muted-foreground">rows affected</span>
				</p>
			</div>
		{:else if !running}
			<div
				class="flex h-full flex-col items-center justify-center gap-3 px-6 text-[13px] text-muted-foreground"
			>
				<span
					class="mb-1 flex size-10 items-center justify-center rounded-xl border bg-chrome text-muted-foreground"
					aria-hidden="true"
				>
					<TableIcon class="size-5" />
				</span>
				<p class="text-sm text-ink-2">Run a query to see results</p>
				<div class="grid grid-cols-[max-content_max-content] items-center gap-x-3 gap-y-2">
					<kbd class="key justify-self-end">⌘ ↵</kbd>
					<span>run the statement at the cursor</span>
					<kbd class="key justify-self-end">⇧ ⌘ ↵</kbd>
					<span>run the whole script</span>
				</div>
			</div>
		{/if}
	</div>
</div>
