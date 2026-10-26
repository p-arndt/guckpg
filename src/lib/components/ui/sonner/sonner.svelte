<script lang="ts">
	import { mode } from 'mode-watcher';
	import { Toaster as Sonner, type ToasterProps as SonnerProps } from 'svelte-sonner';
	import Loader2Icon from '@lucide/svelte/icons/loader-2';
	import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
	import OctagonXIcon from '@lucide/svelte/icons/octagon-x';
	import InfoIcon from '@lucide/svelte/icons/info';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

	let { ...restProps }: SonnerProps = $props();
</script>

<Sonner
	theme={mode.current}
	class="toaster group"
	style="--normal-bg: var(--color-paper-2); --normal-text: var(--color-ink); --normal-border: var(--color-rule-2); --success-bg: var(--color-ok-soft); --success-text: var(--color-ink); --success-border: color-mix(in oklab, var(--color-ok) 45%, transparent); --error-bg: var(--color-bad-soft); --error-text: var(--color-ink); --error-border: color-mix(in oklab, var(--color-bad) 45%, transparent); --info-bg: var(--color-cool-soft); --info-text: var(--color-ink); --info-border: color-mix(in oklab, var(--color-cool) 45%, transparent); --warning-bg: var(--color-accent-soft); --warning-text: var(--color-ink); --warning-border: color-mix(in oklab, var(--color-accent) 45%, transparent); --border-radius: var(--radius-card); font-family: var(--font-body);"
	toastOptions={{
		classes: {
			toast: 'cn-toast'
		}
	}}
	{...restProps}
>
	{#snippet loadingIcon()}
		<Loader2Icon class="size-4 animate-spin" />
	{/snippet}
	{#snippet successIcon()}
		<CircleCheckIcon class="size-4" />
	{/snippet}
	{#snippet errorIcon()}
		<OctagonXIcon class="size-4" />
	{/snippet}
	{#snippet infoIcon()}
		<InfoIcon class="size-4" />
	{/snippet}
	{#snippet warningIcon()}
		<TriangleAlertIcon class="size-4" />
	{/snippet}
</Sonner>
