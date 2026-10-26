<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { ClassValue } from 'svelte/elements';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import SqlBlock from './SqlBlock.svelte';

	interface Props {
		open: boolean;
		title: string;
		description: Snippet;
		/** The script under review, already joined into one string. */
		sql: string;
		confirmLabel: string;
		onConfirm: () => void;
		onClose: () => void;
		/** While true the dialog cannot be dismissed and both buttons are disabled. */
		busy?: boolean;
		/** Only disables the confirm button, for callers that close the dialog before running. */
		confirmDisabled?: boolean;
		error?: string | null;
		destructive?: boolean;
		cancelLabel?: string;
		sqlClass?: ClassValue;
		/** Shown between the header and the SQL, e.g. a data-loss warning. */
		children?: Snippet;
	}
	let {
		open,
		title,
		description,
		sql,
		confirmLabel,
		onConfirm,
		onClose,
		busy = false,
		confirmDisabled = false,
		error = null,
		destructive = false,
		cancelLabel = 'Cancel',
		sqlClass = 'measured max-h-[55vh] overflow-auto rounded-lg border bg-background p-3 text-xs whitespace-pre-wrap select-text',
		children
	}: Props = $props();
</script>

<Dialog.Root
	{open}
	onOpenChange={(next) => {
		if (!next && !busy) onClose();
	}}
>
	<Dialog.Content class="sm:max-w-3xl" showCloseButton={!busy}>
		<Dialog.Header>
			<Dialog.Title>{title}</Dialog.Title>
			<Dialog.Description>{@render description()}</Dialog.Description>
		</Dialog.Header>
		{@render children?.()}
		{#if error}<p role="alert" class="text-xs text-bad">{error}</p>{/if}
		<SqlBlock {sql} class={sqlClass} />
		<Dialog.Footer>
			<Button variant="outline" disabled={busy} onclick={onClose}>{cancelLabel}</Button>
			<Button
				variant={destructive ? 'destructive' : 'default'}
				disabled={busy || confirmDisabled}
				onclick={onConfirm}
			>
				{#if busy}<LoaderIcon class="animate-spin" />{/if}
				{confirmLabel}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
