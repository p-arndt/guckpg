<script lang="ts">
	import type { QueryTab } from '#lib/state/tabs.svelte.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import QueryTabView from '../QueryTabView.svelte';
	let { tab }: { tab: QueryTab } = $props();
</script>

<Tooltip.Provider>
	<QueryTabView {tab} />
</Tooltip.Provider>
