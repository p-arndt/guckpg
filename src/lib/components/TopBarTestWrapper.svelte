<script lang="ts">
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import TopBar from './TopBar.svelte';

	let { sidebarOpen, onToggleSidebar }: { sidebarOpen: boolean; onToggleSidebar: () => void } =
		$props();
</script>

<Tooltip.Provider>
	<TopBar
		onNewQuery={() => {}}
		onOpenPalette={() => {}}
		onToggleAi={() => {}}
		aiOpen={false}
		{sidebarOpen}
		{onToggleSidebar}
	/>
</Tooltip.Provider>
