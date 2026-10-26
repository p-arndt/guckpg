<script lang="ts" module>
	import type { RowField } from '#lib/row-form.js';
	import type { NewCell } from '#lib/state/edits.svelte.js';
	export interface RowViewing {
		key: string;
		number: number;
		position: number;
		total: number;
		isNew: boolean;
		deleted: boolean;
		fields: RowField[];
		apply: (changes: { column: number; value: NewCell }[]) => void;
		navigate: (delta: number) => void;
	}
</script>

<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { confirmation } from '#lib/state/confirm.svelte.js';
	import { rowChanges, formattedJson } from '#lib/row-form.js';
	import { toast } from 'svelte-sonner';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	let { viewing = $bindable(null) }: { viewing: RowViewing | null } = $props();
	let values = $state<NewCell[]>([]);
	let lastText = $state<Record<number, string>>({});
	let changed = $derived(viewing ? rowChanges(viewing.fields, values, viewing.isNew) : []);
	$effect(() => {
		values = viewing?.fields.map((field) => field.value) ?? [];
		lastText = {};
	});
	function stage() {
		if (viewing && changed.length) viewing.apply(changed);
	}
	function done() {
		stage();
		viewing = null;
	}
	function navigate(delta: number) {
		stage();
		viewing?.navigate(delta);
	}
	async function close(open: boolean) {
		if (open) return;
		if (
			changed.length &&
			!(await confirmation.ask({
				title: 'Discard this row’s form edits?',
				description:
					'These edits have not been added to the grid. Changes from previous rows remain pending in the grid.',
				confirmLabel: 'Discard row edits',
				destructive: true
			}))
		)
			return;
		viewing = null;
	}
	function mode(index: number, next: string) {
		if (typeof values[index] === 'string') lastText[index] = values[index];
		values[index] =
			next === 'null' ? null : next === 'default' ? undefined : (lastText[index] ?? '');
	}
	function format(index: number) {
		const result = formattedJson(values[index] ?? '');
		if (result === null) toast.error('This value is not valid JSON');
		else values[index] = result;
	}
</script>

<Dialog.Root open={viewing !== null} onOpenChange={close}>
	<Dialog.Content
		class="flex h-[85vh] w-[min(1050px,95vw)] max-w-none flex-col gap-3 sm:max-w-none"
		onkeydown={(event) => {
			event.stopPropagation();
			if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
				event.preventDefault();
				done();
			}
			if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's')
				event.preventDefault();
		}}
	>
		<Dialog.Header>
			<Dialog.Title
				>{viewing?.isNew ? 'New row' : `Row ${viewing?.number ?? ''}`} · Form</Dialog.Title
			>
			<Dialog.Description>
				{viewing?.deleted
					? 'Marked for deletion. Restore the row in the grid to edit it.'
					: viewing?.fields.some((field) => field.editable)
						? 'Apply or navigate to add edits to the grid. Save in the grid to write them to the database.'
						: 'Read-only row. All returned columns are shown below.'}
			</Dialog.Description>
		</Dialog.Header>
		<div class="flex items-center gap-2 border-b pb-3">
			<Button
				variant="outline"
				size="sm"
				disabled={!viewing || viewing.position === 0}
				onclick={() => navigate(-1)}
				aria-label="Previous row"><ChevronLeftIcon />Previous</Button
			>
			<span class="measured text-xs text-muted-foreground"
				>{(viewing?.position ?? 0) + 1} / {viewing?.total ?? 0} loaded rows</span
			>
			<Button
				variant="outline"
				size="sm"
				disabled={!viewing || viewing.position + 1 >= viewing.total}
				onclick={() => navigate(1)}
				aria-label="Next row">Next<ChevronRightIcon /></Button
			>
			<span class="ml-auto text-xs text-muted-foreground">{viewing?.fields.length ?? 0} fields</span
			>
		</div>
		<div class="min-h-0 flex-1 overflow-auto pr-2">
			{#each viewing?.fields ?? [] as field, index (index)}
				<div class="grid gap-2 border-b py-4 sm:grid-cols-[200px_minmax(0,1fr)]">
					<div class="min-w-0">
						<label for={`row-field-${index}`} class="flex items-center gap-2 font-medium break-all"
							>{#if field.primaryKey}<KeyRoundIcon
									class="size-3.5 shrink-0 text-cool"
									aria-label="Primary key"
								/>{/if}{field.name}</label
						>
						<p class="mt-1 text-xs break-words text-muted-foreground">
							{field.type || 'Result column'}{!field.editable ? ' · read-only' : ''}
						</p>
					</div>
					<div class="min-w-0 space-y-2">
						{#if field.editable}
							<div class="flex items-center gap-2">
								<select
									class="h-7 rounded-md border bg-background px-2 text-xs"
									aria-label={`${field.name} value mode`}
									value={values[index] === null
										? 'null'
										: values[index] === undefined
											? 'default'
											: 'value'}
									onchange={(event) => mode(index, event.currentTarget.value)}
								>
									<option value="value">Value</option><option value="null">NULL</option
									>{#if viewing?.isNew}<option value="default">DEFAULT</option>{/if}
								</select>
								{#if /json/i.test(field.type) && typeof values[index] === 'string'}<Button
										size="xs"
										variant="ghost"
										onclick={() => format(index)}>Format JSON</Button
									>{/if}
							</div>
							{#if typeof values[index] === 'string'}
								{#if /^(bool|boolean)$/i.test(field.type)}
									<select
										id={`row-field-${index}`}
										class="w-full rounded-md border bg-background p-2"
										bind:value={values[index]}
										><option value={values[index]}>{values[index]}</option><option value="true"
											>true</option
										><option value="false">false</option></select
									>
								{:else}
									<textarea
										id={`row-field-${index}`}
										class="min-h-20 w-full resize-y rounded-lg border bg-background p-3 font-mono text-sm"
										rows={/json/i.test(field.type) || (values[index]?.length ?? 0) > 160 ? 6 : 2}
										spellcheck={false}
										value={values[index]}
										oninput={(event) => (values[index] = event.currentTarget.value)}></textarea>
								{/if}
							{:else}<div
									id={`row-field-${index}`}
									class="rounded-lg border bg-muted/30 p-3 font-mono text-sm text-muted-foreground"
								>
									{values[index] === null ? 'NULL' : 'DEFAULT — database default'}
								</div>{/if}
						{:else}
							<pre
								id={`row-field-${index}`}
								class="min-h-10 overflow-auto rounded-lg border bg-muted/20 p-3 font-mono text-sm break-words whitespace-pre-wrap select-text">{field.value ===
								null
									? 'NULL'
									: field.value === undefined
										? 'DEFAULT'
										: /json/i.test(field.type)
											? (formattedJson(field.value) ?? field.value)
											: field.value}</pre>
						{/if}
					</div>
				</div>
			{/each}
		</div>
		<Dialog.Footer class="items-center">
			<span class="mr-auto text-xs text-muted-foreground"
				>{changed.length
					? `${changed.length} changed fields in this form`
					: 'No additional form edits'}</span
			>
			<Button variant="outline" onclick={() => close(false)}
				>{changed.length ? 'Cancel' : 'Close'}</Button
			>
			{#if viewing?.fields.some((field) => field.editable)}<Button onclick={done}
					>{changed.length ? 'Apply to grid' : 'Done'}</Button
				>{/if}
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
