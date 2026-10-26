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
				<ShieldQuestionIcon class="size-5" /> Unknown SSH server
			</AlertDialog.Title>
			<AlertDialog.Description>
				guckpg has never seen the host key of
				<span class="font-medium text-foreground">{pending?.host}:{pending?.port}</span>. Compare
				the fingerprint with the server (<code class="font-mono text-xs"
					>ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub</code
				>) before trusting it.
			</AlertDialog.Description>
		</AlertDialog.Header>
		<code class="rounded-lg border bg-background p-2 font-mono text-xs break-all select-text">
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
