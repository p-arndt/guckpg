<script lang="ts">
	import type { ItemStatus } from '#lib/structure.js';

	let { status }: { status: ItemStatus } = $props();
</script>

{#if status !== 'same'}<span class="chip {status === 'new' ? 'chip-ok' : 'chip-signal'}"
		>{status === 'new' ? 'New' : 'Changed'}</span
	>{/if}
