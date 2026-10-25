<script lang="ts" module>
	export interface Viewing {
		column: string;
		value: string | null;
		/** Present when the value may be changed. */
		save?: (value: string | null) => void;
	}
</script>

<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { toast } from 'svelte-sonner';

	interface Props {
		viewing: Viewing | null;
	}

	let { viewing = $bindable() }: Props = $props();

	let draft = $state('');

	/** JSON gets pretty-printed when only reading it. */
	function pretty(v: string): string {
		const trimmed = v.trimStart();
		if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
			try {
				return JSON.stringify(JSON.parse(v), null, 2);
			} catch {
				return v;
			}
		}
		return v;
	}

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
			<Dialog.Title class="font-mono">{viewing?.column}</Dialog.Title>
			<Dialog.Description>
				{viewing?.value === null ? 'NULL' : `${viewing?.value.length ?? 0} characters`}
			</Dialog.Description>
		</Dialog.Header>
		{#if viewing?.save}
			<textarea
				bind:value={draft}
				spellcheck={false}
				class="h-[50vh] w-full resize-none rounded-md border bg-muted/40 p-3 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
				onkeydown={(e) => {
					if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') save(draft);
				}}></textarea>
		{:else}
			<pre
				class="max-h-[60vh] overflow-auto rounded-md border bg-muted/40 p-3 font-mono text-xs whitespace-pre-wrap select-text">{draft}</pre>
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
				<Button variant="outline" onclick={() => save(null)}>Set NULL</Button>
				<Button onclick={() => save(draft)}>Apply <span class="opacity-60">⌘↵</span></Button>
			{/if}
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
