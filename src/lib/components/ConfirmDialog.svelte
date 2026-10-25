<script lang="ts">
	import * as AlertDialog from '#lib/components/ui/alert-dialog/index.js';
	import { confirmation } from '#lib/state/confirm.svelte.js';

	const pending = $derived(confirmation.pending);
</script>

<AlertDialog.Root
	open={pending !== null}
	onOpenChange={(open) => {
		if (!open) confirmation.answer(false);
	}}
>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{pending?.title}</AlertDialog.Title>
			<AlertDialog.Description>{pending?.description}</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel onclick={() => confirmation.answer(false)}>Cancel</AlertDialog.Cancel>
			<AlertDialog.Action
				variant={pending?.destructive ? 'destructive' : 'default'}
				onclick={() => confirmation.answer(true)}
			>
				{pending?.confirmLabel}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
