<script lang="ts">
	import type { ClassValue } from 'svelte/elements';
	import { highlightSql } from '#lib/highlight.js';

	interface Props {
		sql: string;
		class?: ClassValue;
		/** Wrap the tokens in <code>, for prose contexts like chat and markdown. */
		code?: boolean;
	}
	let { sql, class: className, code = false }: Props = $props();

	let tokens = $derived(highlightSql(sql));
</script>

{#snippet highlighted()}{#each tokens as token, t (t)}{#if token.classes}<span class={token.classes}
				>{token.text}</span
			>{:else}{token.text}{/if}{/each}{/snippet}

<pre class={className}>{#if code}<code>{@render highlighted()}</code
		>{:else}{@render highlighted()}{/if}</pre>
