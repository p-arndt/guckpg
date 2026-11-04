<script lang="ts">
	let { value = $bindable('') }: { value: string } = $props();
	export function selection() {
		return null;
	}
	export function cursor() {
		return 0;
	}
	export function markError() {}
</script>

<textarea bind:value aria-label="SQL"></textarea>
