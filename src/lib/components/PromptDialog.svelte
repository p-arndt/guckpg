<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { prompt } from '#lib/state/prompt.svelte.js';

	const pending = $derived(prompt.pending);
	let value = $derived(pending?.initial ?? '');

	function submit(e: SubmitEvent) {
		e.preventDefault();
		prompt.answer(value);
	}
</script>

<Dialog.Root
	open={pending !== null}
	onOpenChange={(open) => {
		if (!open) prompt.answer(null);
	}}
>
	<Dialog.Content class="sm:max-w-sm">
		<form onsubmit={submit} class="grid gap-4">
			<Dialog.Header>
				<Dialog.Title>{pending?.title}</Dialog.Title>
			</Dialog.Header>
			<div class="grid gap-1.5">
				<Label for="prompt-value">{pending?.label}</Label>
				<Input
					id="prompt-value"
					bind:value
					autofocus
					autocomplete="off"
					spellcheck={false}
					onfocus={(e) => e.currentTarget.select()}
				/>
			</div>
			<Dialog.Footer class="gap-2">
				<Button type="button" variant="outline" onclick={() => prompt.answer(null)}>Cancel</Button>
				<Button type="submit" disabled={!value.trim()}>{pending?.confirmLabel ?? 'OK'}</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
