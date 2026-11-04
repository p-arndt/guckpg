<script lang="ts">
	let { onopen }: { onopen: (sql: string) => void } = $props();
</script>

<button onclick={() => onopen('SELECT 1;')}>Edit SQL</button>
