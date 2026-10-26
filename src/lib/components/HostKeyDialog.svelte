<script lang="ts">
	import ShieldQuestionIcon from '@lucide/svelte/icons/shield-question';
	import * as AlertDialog from '#lib/components/ui/alert-dialog/index.js';
	import { hostKeyPrompt } from '#lib/state/host-key.svelte.js';

	const pending = $derived(hostKeyPrompt.pending);
</script>

<AlertDialog.Root
	open={pending !== null}
	onOpenChange={(open) => {
		if (!open) hostKeyPrompt.answer(false);
	}}
>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title class="flex items-center gap-2">
				<ShieldQuestionIcon class="size-4 text-muted-foreground" /> Unknown SSH server
			</AlertDialog.Title>
			<AlertDialog.Description>
				First connection to
				<span class="font-medium text-foreground">{pending?.host}:{pending?.port}</span>. Check the
				fingerprint on the server with
				<code class="font-mono text-xs text-ink-2"
					>ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub</code
				> before trusting it.
			</AlertDialog.Description>
		</AlertDialog.Header>
		<code
			class="measured rounded-md border bg-background px-2.5 py-2 text-xs leading-relaxed break-all text-ink-2 select-text"
		>
			{pending?.fingerprint}
		</code>
		<AlertDialog.Footer>
			<AlertDialog.Cancel onclick={() => hostKeyPrompt.answer(false)}>Cancel</AlertDialog.Cancel>
			<AlertDialog.Action onclick={() => hostKeyPrompt.answer(true)}>
				Trust and connect
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
