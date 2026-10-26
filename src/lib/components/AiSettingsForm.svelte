<script lang="ts">
	import { plural } from '#lib/format.js';
	import { onMount } from 'svelte';
	import {
		aiApi,
		defaultAiConfig,
		modelCredentials,
		settingsPayload,
		withTimeout,
		type AiConfig,
		type AiProvider
	} from '#lib/ai.js';
	import { errorMessage } from '#lib/api.js';
	import { ai } from '#lib/state/ai.svelte.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import XIcon from '@lucide/svelte/icons/x';

	interface Props {
		/** Whether the AI panel is visible; hiding it forgets typed secrets. */
		open: boolean;
		/** Leaves the form, after saving or without saving. */
		onClose: () => void;
	}
	let { open, onClose }: Props = $props();

	const PROVIDERS: Record<AiProvider, string> = {
		ollama: 'Ollama',
		openaiResponses: 'OpenAI Responses API',
		openaiCompatible: 'OpenAI-compatible'
	};
	const AUTH_MODES: Record<AiConfig['authMode'], string> = {
		none: 'None',
		bearer: 'Bearer token (API key)',
		basic: 'Basic Auth'
	};
	const MODELS_TIMEOUT_MS = 15000;

	/** The form is mounted per visit, so unsaved edits are dropped by closing it. */
	let config = $state({ ...ai.settings.config });
	/** Returns `value`; reading `key` makes a writable `$derived` fall back to it on change. */
	const resetOn = <T,>(key: unknown, value: T) => (void key, value);
	let apiKey = $derived(resetOn(open, ''));
	let basicUsername = $derived(resetOn(open, ''));
	let basicPassword = $derived(resetOn(open, ''));
	let clearBasic = $state(false);
	let clearKey = $state(false);
	let saving = $state(false);
	let error = $state('');
	let models = $state<string[]>([]);
	let modelsLoaded = $state(false);
	let loadingModels = $state(false);
	let modelRequest = 0;

	const draft = () => ({ apiKey, clearKey, basicUsername, basicPassword, clearBasic });
	const sameEndpoint = $derived(
		config.provider === ai.settings.config.provider &&
			config.baseUrl.trim() === ai.settings.config.baseUrl
	);

	onMount(() => {
		if (config.provider === 'ollama') void loadModels();
		return () => modelRequest++;
	});

	async function loadModels() {
		const request = ++modelRequest;
		loadingModels = true;
		modelsLoaded = false;
		models = [];
		error = '';
		try {
			const { apiKey, basicAuth } = modelCredentials(config, draft());
			const result = await withTimeout(
				aiApi.models({ ...config, baseUrl: config.baseUrl.trim() }, apiKey, basicAuth),
				MODELS_TIMEOUT_MS,
				'Loading models timed out. Check the URL, connection, and authentication.'
			);
			if (request === modelRequest) {
				models = result;
				modelsLoaded = true;
			}
		} catch (e) {
			if (request === modelRequest) error = errorMessage(e);
		} finally {
			if (request === modelRequest) loadingModels = false;
		}
	}

	function invalidateModels() {
		modelRequest++;
		models = [];
		modelsLoaded = false;
		loadingModels = false;
		error = '';
	}

	function changeProvider(provider: string) {
		invalidateModels();
		config = defaultAiConfig(provider as AiProvider);
		apiKey = '';
		clearKey = false;
		basicUsername = '';
		basicPassword = '';
		clearBasic = false;
		ai.reset();
	}

	async function save() {
		saving = true;
		error = '';
		ai.reset();
		try {
			const payload = settingsPayload(config, draft(), sameEndpoint);
			ai.settings = await aiApi.saveSettings(payload.config, payload.apiKey, payload.basicAuth);
			ai.loaded = true;
			onClose();
		} catch (e) {
			error = errorMessage(e);
		} finally {
			saving = false;
		}
	}
</script>

<form
	class="glass-card mx-2 mb-2 flex min-h-0 flex-1 flex-col rounded-xl border bg-background shadow-panel"
	onsubmit={(e) => {
		e.preventDefault();
		void save();
	}}
>
	<div class="flex items-center justify-between border-b px-4 py-3">
		<div>
			<h3 class="font-medium">Assistant settings</h3>
			<p class="text-xs text-muted-foreground">Configure the model and its connection</p>
		</div>
		<Button
			type="button"
			size="icon-sm"
			variant="ghost"
			aria-label="Close settings"
			onclick={onClose}><XIcon /></Button
		>
	</div>
	<div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
		<div class="flex flex-col gap-1.5">
			<label for="ai-provider" class="text-[13px] font-medium text-muted-foreground">Provider</label
			>
			<Select.Root
				type="single"
				value={config.provider}
				onValueChange={changeProvider}
				disabled={saving}
			>
				<Select.Trigger id="ai-provider" class="w-full">
					<span class="truncate">{PROVIDERS[config.provider]}</span>
				</Select.Trigger>
				<Select.Content>
					{#each Object.entries(PROVIDERS) as [value, label] (value)}
						<Select.Item {value} {label} />
					{/each}
				</Select.Content>
			</Select.Root>
		</div>
		<div class="flex flex-col gap-1.5">
			<label for="ai-base-url" class="text-[13px] font-medium text-muted-foreground">Base URL</label
			>
			<Input
				id="ai-base-url"
				bind:value={config.baseUrl}
				oninput={invalidateModels}
				required
				disabled={saving}
				aria-describedby="ai-base-url-hint"
			/>
			<p id="ai-base-url-hint" class="text-xs text-muted-foreground">
				Use https. Plain http only works for localhost.
			</p>
		</div>
		<div class="flex flex-col gap-1.5">
			<label for="ai-model" class="text-[13px] font-medium text-muted-foreground">Model</label>
			{#if config.provider === 'ollama'}
				<div class="flex gap-2">
					<Select.Root
						type="single"
						value={models.includes(config.model) ? config.model : ''}
						onValueChange={(model) => {
							if (model) config.model = model;
						}}
						disabled={!models.length || saving}
					>
						<Select.Trigger aria-label="Available Ollama models" class="min-w-0 flex-1">
							<span class="truncate"
								>{models.includes(config.model)
									? config.model
									: loadingModels
										? 'Loading models…'
										: modelsLoaded && !models.length
											? 'No models found'
											: 'Select a model'}</span
							>
						</Select.Trigger>
						<Select.Content>
							{#each models as model (model)}<Select.Item value={model} label={model} />{/each}
						</Select.Content>
					</Select.Root>
					<Button
						type="button"
						size="sm"
						variant="outline"
						disabled={loadingModels || saving}
						onclick={loadModels}>{loadingModels ? 'Loading…' : 'Refresh'}</Button
					>
				</div>
				<p role="status" class="text-xs text-muted-foreground">
					{loadingModels
						? 'Connecting to Ollama…'
						: modelsLoaded
							? `${plural(models.length, 'model')} available`
							: 'Uses the credentials below or saved credentials for this URL.'}
				</p>
			{/if}
			<Input
				id="ai-model"
				bind:value={config.model}
				placeholder="Or enter a model name"
				required
				disabled={saving}
			/>
		</div>
		<div class="flex flex-col gap-1.5">
			<label for="ai-auth-mode" class="text-[13px] font-medium text-muted-foreground"
				>Authentication</label
			>
			<Select.Root
				type="single"
				value={config.authMode}
				onValueChange={(mode) => {
					config.authMode = mode as AiConfig['authMode'];
					invalidateModels();
				}}
				disabled={saving}
			>
				<Select.Trigger id="ai-auth-mode" class="w-full">
					<span class="truncate">{AUTH_MODES[config.authMode]}</span>
				</Select.Trigger>
				<Select.Content>
					{#each Object.entries(AUTH_MODES) as [value, label] (value)}
						<Select.Item {value} {label} />
					{/each}
				</Select.Content>
			</Select.Root>
		</div>
		{#if config.authMode === 'bearer'}
			<div class="flex flex-col gap-1.5">
				<label for="ai-api-key" class="text-[13px] font-medium text-muted-foreground">API key</label
				>
				<Input
					id="ai-api-key"
					type="password"
					autocomplete="off"
					bind:value={apiKey}
					oninput={invalidateModels}
					placeholder="Leave blank to keep saved key"
					disabled={saving || (clearKey && sameEndpoint)}
				/>
				<p class="text-xs text-muted-foreground">
					{sameEndpoint
						? ai.settings.hasApiKey
							? 'API key saved in keychain.'
							: 'No API key saved.'
						: 'Save settings to check the key for this endpoint.'}
				</p>
			</div>
			{#if sameEndpoint && ai.settings.hasApiKey}
				<label class="flex items-center gap-2 text-[13px]"
					><input
						type="checkbox"
						class="accent-signal"
						bind:checked={clearKey}
						disabled={saving}
					/>Delete saved API key on save</label
				>
			{/if}
		{:else if config.authMode === 'basic'}
			<div class="grid grid-cols-2 gap-2">
				<div class="flex min-w-0 flex-col gap-1.5">
					<label for="ai-basic-username" class="text-[13px] font-medium text-muted-foreground"
						>Username</label
					>
					<Input
						id="ai-basic-username"
						autocomplete="off"
						bind:value={basicUsername}
						oninput={invalidateModels}
						placeholder="Blank keeps saved"
						disabled={saving || (clearBasic && sameEndpoint)}
					/>
				</div>
				<div class="flex min-w-0 flex-col gap-1.5">
					<label for="ai-basic-password" class="text-[13px] font-medium text-muted-foreground"
						>Password</label
					>
					<Input
						id="ai-basic-password"
						type="password"
						autocomplete="off"
						bind:value={basicPassword}
						oninput={invalidateModels}
						placeholder="Blank keeps saved"
						disabled={saving || (clearBasic && sameEndpoint)}
					/>
				</div>
			</div>
			<p class="-mt-1.5 text-xs text-muted-foreground">
				{sameEndpoint
					? ai.settings.hasBasicAuth
						? 'Basic Auth credentials saved in keychain.'
						: 'No Basic Auth credentials saved.'
					: 'Save settings to check the credentials for this endpoint.'}
			</p>
			{#if sameEndpoint && ai.settings.hasBasicAuth}
				<label class="flex items-center gap-2 text-[13px]"
					><input
						type="checkbox"
						class="accent-signal"
						bind:checked={clearBasic}
						disabled={saving}
					/>Delete saved Basic Auth on save</label
				>
			{/if}
		{/if}
		{#if error}<p role="alert" class="rounded-lg bg-bad-soft px-3 py-2 text-[13px] text-bad">
				{error}
			</p>{/if}
	</div>
	<div class="flex shrink-0 justify-end gap-2 border-t px-4 py-3">
		<Button type="button" size="sm" variant="ghost" onclick={onClose}>Cancel</Button>
		<Button type="submit" size="sm" disabled={saving}>{saving ? 'Saving…' : 'Save settings'}</Button
		>
	</div>
</form>
