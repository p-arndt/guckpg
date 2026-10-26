<script lang="ts">
	import TrashIcon from '@lucide/svelte/icons/trash';
	import UndoIcon from '@lucide/svelte/icons/undo-2';
	import { Button } from '#lib/components/ui/button/index.js';

	interface Props {
		label: string;
		detail: string;
		disabled: boolean;
		onrestore: () => void;
	}
	let { label, detail, disabled, onrestore }: Props = $props();
</script>

<div
	class="flex items-center gap-3 rounded-lg border border-dashed border-[color-mix(in_oklab,var(--color-bad),transparent_60%)] bg-bad-soft px-3 py-1.5 text-xs"
>
	<TrashIcon class="size-3.5 text-bad" />
	<span class="measured text-muted-foreground line-through">{label}</span>
	<span class="measured truncate text-muted-foreground">{detail}</span>
	<span class="flex-1"></span>
	<span class="text-bad">Will be dropped</span>
	<Button size="xs" variant="ghost" {disabled} onclick={onrestore}><UndoIcon /> Restore</Button>
</div>
