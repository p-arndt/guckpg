<script lang="ts">
	import FileInputIcon from '@lucide/svelte/icons/file-input';
	import { invoke, errorMessage } from '#lib/api.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';

	interface ImportResult {
		imported: number;
		skipped: number;
		warnings: string[];
	}

	let open = $state(false);
	let busy = $state(false);
	let error = $state('');
	let result = $state<ImportResult | null>(null);
	let input: HTMLInputElement;

	async function importFile(event: Event) {
		const element = event.currentTarget as HTMLInputElement;
		const file = element.files?.[0];
		element.value = '';
		if (!file || busy) return;
		error = '';
		result = null;
		if (file.size > 16 * 1024 * 1024) {
			error = 'The file exceeds the 16 MiB limit.';
			return;
		}
		busy = true;
		try {
			const xml = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
			result = await invoke<ImportResult>('import_navicat', { xml });
		} catch (e) {
			error = errorMessage(e);
		} finally {
			try {
				await connections.load();
			} catch (e) {
				error = `${error ? `${error} ` : ''}Could not refresh connections: ${errorMessage(e)}`;
			}
			busy = false;
		}
	}
</script>

<Button
	size="icon-xs"
	variant="ghost"
	title="Import Navicat connections"
	aria-label="Import Navicat connections"
	onclick={() => (open = true)}
>
	<FileInputIcon />
</Button>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-md">
		<Dialog.Header>
			<Dialog.Title>Import from Navicat</Dialog.Title>
			<Dialog.Description>
				Choose a .ncx file exported from Navicat. PostgreSQL connections will be added to your
				connection list. Export passwords in Navicat if you want to import them too.
			</Dialog.Description>
		</Dialog.Header>
		<input bind:this={input} type="file" accept=".ncx" class="hidden" onchange={importFile} />
		<div class="flex flex-col gap-3">
			<Button variant="outline" disabled={busy} onclick={() => input.click()}>
				<FileInputIcon />
				{busy ? 'Importing…' : 'Choose .ncx file'}
			</Button>
			{#if result}
				<div role="status" class="text-sm text-ink-2">
					<p>Imported {result.imported} connections; skipped {result.skipped}.</p>
					{#if result.warnings.length}
						<ul class="mt-2 list-disc space-y-1 pl-5">
							{#each result.warnings as warning, i (i)}<li>{warning}</li>{/each}
						</ul>
					{/if}
				</div>
			{/if}
			{#if error}<p role="alert" class="text-sm text-bad">{error}</p>{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>
