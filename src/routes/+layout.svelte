<script lang="ts">
	import './layout.css';
	import { isTauri } from '@tauri-apps/api/core';
	import { getCurrentWebview } from '@tauri-apps/api/webview';
	import { zoomForKey } from '#lib/zoom.js';
	import { ModeWatcher } from 'mode-watcher';
	import { Toaster } from '#lib/components/ui/sonner/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
	let zoom = 1;

	function onZoomKeydown(event: KeyboardEvent) {
		if (!isTauri()) return;
		const next = zoomForKey(event, zoom);
		if (next === null) return;
		event.preventDefault();
		zoom = next;
		void getCurrentWebview().setZoom(zoom);
	}
</script>

<svelte:window onkeydown={onZoomKeydown} />

<ModeWatcher />
<Toaster position="bottom-right" richColors />
<Tooltip.Provider delayDuration={400}>
	{@render children()}
</Tooltip.Provider>
