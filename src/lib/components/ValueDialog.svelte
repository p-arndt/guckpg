<script lang="ts" module>
	export interface Viewing {
		column: string;
		value: string | null;
		/** Column type, when known; json columns get the JSON editor. */
		type?: string;
		/** Present when the value may be changed. */
		save?: (value: string | null) => void;
	}
</script>

<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { toast } from 'svelte-sonner';
	import { isJsonType } from '#lib/highlight.js';
	import { formattedJson } from '#lib/row-form.js';
	import JsonEditor from './JsonEditor.svelte';

	interface Props {
		viewing: Viewing | null;
	}

	let { viewing = $bindable() }: Props = $props();

	let draft = $state('');

	/** JSON gets pretty-printed when only reading it. */
	function pretty(v: string): string {
		return formattedJson(v) ?? v;
	}

	function looksLikeJson(v: string): boolean {
		const trimmed = v.trimStart();
		return (trimmed.startsWith('{') || trimmed.startsWith('[')) && formattedJson(v) !== null;
	}

	const isJson = $derived(
		viewing !== null &&
			viewing.value !== null &&
			(isJsonType(viewing.type) || (!viewing.type && looksLikeJson(viewing.value)))
	);

	// Editing keeps the exact text: reformatting would turn into a change.
	$effect(() => {
		if (!viewing) return;
		const v = viewing.value ?? '';
		draft = viewing.save ? v : pretty(v);
	});

	function save(value: string | null) {
		viewing?.save?.(value);
		viewing = null;
	}
</script>

<Dialog.Root
	open={viewing !== null}
	onOpenChange={(open) => {
		if (!open) viewing = null;
	}}
>
	<Dialog.Content class="sm:max-w-3xl">
		<Dialog.Header>
			<Dialog.Title>{viewing?.column}</Dialog.Title>
			<Dialog.Description>
				{viewing?.value === null ? 'NULL' : `${viewing?.value.length ?? 0} characters`}
			</Dialog.Description>
		</Dialog.Header>
		{#if isJson}
			<JsonEditor
				value={draft}
				label="Value"
				readonly={!viewing?.save}
				maxHeight="60vh"
				onchange={(next) => (draft = next)}
				onkeydown={(e) => {
					if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && viewing?.save) save(draft);
				}}
			/>
		{:else if viewing?.save}
			<textarea
				bind:value={draft}
				spellcheck={false}
				aria-label="Value"
				class="h-[50vh] w-full resize-none rounded-lg border border-input bg-background p-3 font-mono text-xs"
				onkeydown={(e) => {
					if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') save(draft);
				}}></textarea>
		{:else}
			<pre
				class="max-h-[60vh] overflow-auto rounded-lg border bg-background p-3 font-mono text-xs whitespace-pre-wrap select-text">{draft}</pre>
		{/if}
		<Dialog.Footer>
			<Button
				variant="outline"
				onclick={async () => {
					await navigator.clipboard.writeText(draft);
					toast.success('Copied value');
				}}
			>
				Copy
			</Button>
			{#if viewing?.save}
				{#if isJson}
					<Button
						variant="ghost"
						class="mr-auto"
						onclick={() => {
							const result = formattedJson(draft);
							if (result === null) toast.error('This value is not valid JSON');
							else draft = result;
						}}>Format JSON</Button
					>
				{/if}
				<Button variant="outline" onclick={() => save(null)}>Set NULL</Button>
				<Button onclick={() => save(draft)}
					>Apply <span class="measured opacity-70">⌘↵</span></Button
				>
			{/if}
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
