<script lang="ts">
	import { mode, userPrefersMode, setMode } from 'mode-watcher';
	import Settings2Icon from '@lucide/svelte/icons/settings-2';
	import ImageUpIcon from '@lucide/svelte/icons/image-up';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import BackdropArt from './BackdropArt.svelte';
	import { appearance } from '#lib/state/appearance.svelte.js';
	import type { Accent, Art } from '#lib/appearance.js';

	let open = $state(false);
	let fileError = $state('');
	let fileInput = $state<HTMLInputElement | null>(null);
	const error = $derived(fileError || appearance.error);
	const isDark = $derived(mode.current === 'dark');

	const arts: { value: Art; label: string }[] = [
		{ value: 'aurora', label: 'Aurora' },
		{ value: 'dusk', label: 'Dusk' },
		{ value: 'contours', label: 'Contours' },
		{ value: 'tide', label: 'Tide' },
		{ value: 'nebula', label: 'Nebula' },
		{ value: 'ember', label: 'Ember' }
	];
	const themes = [
		{ value: 'system', label: 'System' },
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' }
	] as const;
	const accents: { value: Accent; label: string; swatch: string }[] = [
		{ value: 'blue', label: 'Blue', swatch: 'oklch(68% 0.15 264)' },
		{ value: 'green', label: 'Green', swatch: 'oklch(76% 0.15 150)' },
		{ value: 'purple', label: 'Purple', swatch: 'oklch(76% 0.15 305)' }
	];

	function chooseImage(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type) || file.size > 2_000_000) {
			fileError = 'Choose a PNG, JPEG or WebP image up to 2 MB.';
			return;
		}
		const reader = new FileReader();
		reader.onload = () => {
			if (typeof reader.result !== 'string') return;
			fileError = '';
			appearance.set({ image: reader.result, background: 'custom' });
		};
		reader.onerror = () => (fileError = 'Could not read the image.');
		reader.readAsDataURL(file);
	}

	function pickPhoto() {
		if (appearance.image) appearance.set({ background: 'custom' });
		else fileInput?.click();
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Trigger>
		{#snippet child({ props })}
			<Button {...props} size="icon-sm" variant="ghost" aria-label="Appearance" title="Appearance"
				><Settings2Icon /></Button
			>
		{/snippet}
	</Dialog.Trigger>
	<Dialog.Content class="sm:max-w-[30rem]">
		<Dialog.Header>
			<Dialog.Title>Appearance</Dialog.Title>
			<Dialog.Description>Changes apply right away.</Dialog.Description>
		</Dialog.Header>

		<div class="flex flex-col gap-6 pt-1">
			<section class="flex flex-col gap-2.5">
				<h3 class="text-[13px] font-medium text-muted-foreground">Theme</h3>
				<div class="segmented" role="radiogroup" aria-label="Theme">
					{#each themes as theme (theme.value)}
						<button
							type="button"
							role="radio"
							aria-checked={userPrefersMode.current === theme.value}
							onclick={() => setMode(theme.value)}>{theme.label}</button
						>
					{/each}
				</div>
			</section>

			<section class="flex flex-col gap-2.5">
				<div class="flex items-baseline justify-between gap-3">
					<h3 class="text-[13px] font-medium text-muted-foreground">Background</h3>
					{#if !isDark}
						<span class="text-xs text-muted-foreground">Shown in dark mode</span>
					{/if}
				</div>
				<div class="grid grid-cols-4 gap-x-2.5 gap-y-3" role="radiogroup" aria-label="Background">
					{#each arts as art (art.value)}
						<button
							type="button"
							role="radio"
							class="art-choice"
							aria-checked={appearance.background === art.value}
							onclick={() => appearance.set({ background: art.value })}
						>
							<BackdropArt variant={art.value} class="art-tile" />
							<span>{art.label}</span>
						</button>
					{/each}
					<button
						type="button"
						role="radio"
						class="art-choice"
						aria-checked={appearance.background === 'custom'}
						onclick={pickPhoto}
					>
						{#if appearance.image}
							<BackdropArt variant="custom" class="art-tile" />
						{:else}
							<span class="art-tile grid place-items-center text-muted-foreground"
								><ImageUpIcon class="size-4" /></span
							>
						{/if}
						<span>Photo</span>
					</button>
					<button
						type="button"
						role="radio"
						class="art-choice"
						aria-checked={appearance.background === 'none'}
						onclick={() => appearance.set({ background: 'none' })}
					>
						<span class="art-tile bg-chrome"></span>
						<span>None</span>
					</button>
				</div>
				{#if appearance.image}
					<button
						type="button"
						class="self-start text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
						onclick={() => fileInput?.click()}>Choose another photo…</button
					>
				{/if}
				<input
					bind:this={fileInput}
					class="hidden"
					type="file"
					accept="image/png,image/jpeg,image/webp"
					onchange={chooseImage}
				/>
			</section>

			<section class="flex flex-col gap-2.5">
				<div class="flex items-baseline justify-between gap-3">
					<label for="appearance-glass" class="text-[13px] font-medium text-muted-foreground"
						>Glass</label
					>
					<span class="measured text-xs text-muted-foreground">{appearance.glass}%</span>
				</div>
				<input
					id="appearance-glass"
					class="slider"
					type="range"
					min="0"
					max="100"
					step="5"
					disabled={!isDark || appearance.background === 'none'}
					style:--value="{appearance.glass}%"
					value={appearance.glass}
					oninput={(e) => appearance.set({ glass: e.currentTarget.valueAsNumber })}
				/>
			</section>

			<section class="flex flex-col gap-2.5">
				<h3 class="text-[13px] font-medium text-muted-foreground">Accent</h3>
				<div class="flex gap-2" role="radiogroup" aria-label="Accent">
					{#each accents as accent (accent.value)}
						<button
							type="button"
							role="radio"
							class="accent-choice"
							aria-checked={appearance.accent === accent.value}
							onclick={() => appearance.set({ accent: accent.value })}
						>
							<span class="size-3 rounded-full" style:background={accent.swatch}></span>
							{accent.label}
						</button>
					{/each}
				</div>
			</section>

			{#if error}<p role="alert" class="text-sm text-bad">{error}</p>{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>
