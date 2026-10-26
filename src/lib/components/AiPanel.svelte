<script lang="ts">
	import { untrack } from 'svelte';
	import { aiApi, defaultAiConfig, type AiProvider } from '#lib/ai.js';
	import { errorMessage } from '#lib/api.js';
	import { ai } from '#lib/state/ai.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import XIcon from '@lucide/svelte/icons/x';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import Settings2Icon from '@lucide/svelte/icons/settings-2';
	import SparklesIcon from '@lucide/svelte/icons/sparkles';
	import SquareIcon from '@lucide/svelte/icons/square';
	import SquarePenIcon from '@lucide/svelte/icons/square-pen';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { isSendKey } from '#lib/composer.js';
	import Markdown from './Markdown.svelte';

	const SUGGESTIONS = [
		'Give me an overview of this schema',
		'Which tables reference each other?',
		'Find tables without a primary key'
	];

	let { open, onClose }: { open: boolean; onClose: () => void } = $props();
	let draft = $state('');
	let config = $state(defaultAiConfig());
	let apiKey = $state('');
	let basicUsername = $state('');
	let basicPassword = $state('');
	let clearBasic = $state(false);
	let clearKey = $state(false);
	let saving = $state(false);
	let settingsOpen = $state(false);
	let settingsError = $state('');
	let loadStarted = false;
	const sameEndpoint = $derived(
		config.provider === ai.settings.config.provider &&
			config.baseUrl.trim() === ai.settings.config.baseUrl
	);
	const context = $derived.by(() => {
		const tab = tabs.active;
		const connection = tab
			? connections.get(tab.connectionId)
			: connections.list.find((c) => connections.status[c.id] === 'connected');
		if (!connection || connections.status[connection.id] !== 'connected') return null;
		return {
			connectionId: connection.id,
			database: tab?.database ?? connections.info[connection.id]?.database ?? connection.database,
			name: connection.name,
			user: connection.user
		};
	});

	async function loadSettings() {
		await ai.load();
		config = { ...ai.settings.config };
		settingsOpen = !config.model;
	}
	$effect(() => {
		const current = context;
		untrack(() =>
			ai.setContext(
				current ? { connectionId: current.connectionId, database: current.database } : null
			)
		);
		draft = '';
	});
	$effect(() => {
		if (!open) {
			apiKey = '';
			basicUsername = '';
			basicPassword = '';
		} else if (!loadStarted) {
			loadStarted = true;
			untrack(() => {
				void loadSettings();
			});
		}
	});

	function changeProvider(event: Event) {
		config = defaultAiConfig((event.target as HTMLSelectElement).value as AiProvider);
		apiKey = '';
		clearKey = false;
		basicUsername = '';
		basicPassword = '';
		clearBasic = false;
		ai.reset();
	}

	async function saveSettings() {
		saving = true;
		settingsError = '';
		ai.reset();
		try {
			ai.settings = await aiApi.saveSettings(
				{ ...config, baseUrl: config.baseUrl.trim(), model: config.model.trim() },
				config.authMode === 'bearer'
					? clearKey && sameEndpoint
						? ''
						: apiKey || undefined
					: undefined,
				config.authMode === 'basic' && clearBasic && sameEndpoint
					? { username: '', password: '' }
					: config.authMode === 'basic' && (basicUsername || basicPassword)
						? { username: basicUsername, password: basicPassword }
						: undefined
			);
			ai.loaded = true;
			config = { ...ai.settings.config };
			apiKey = '';
			clearKey = false;
			basicUsername = '';
			basicPassword = '';
			clearBasic = false;
			settingsOpen = false;
		} catch (e) {
			settingsError = errorMessage(e);
		} finally {
			saving = false;
		}
	}

	const inputEnabled = $derived(!!context && !ai.busy && !saving && !settingsOpen && ai.loaded);
	const canSend = $derived(inputEnabled);

	function send() {
		if (!draft.trim() || !inputEnabled) return;
		const content = draft;
		draft = '';
		atBottom = true;
		void ai.send(content);
	}

	let log = $state<HTMLDivElement | null>(null);
	/** Follow new messages only while the reader is at the end of the log. */
	let atBottom = $state(true);

	function onLogScroll() {
		if (!log) return;
		atBottom = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
	}

	function scrollToEnd(smooth: boolean) {
		if (!log) return;
		const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
		log.scrollTo({ top: log.scrollHeight, behavior: smooth && !reduce ? 'smooth' : 'auto' });
		atBottom = true;
	}

	$effect(() => {
		void ai.messages.length;
		void ai.busy;
		if (untrack(() => atBottom)) scrollToEnd(true);
	});

	// Reopening the panel lands on the latest message.
	$effect(() => {
		if (open) untrack(() => scrollToEnd(false));
	});
</script>

<aside
	class="flex h-full w-full min-w-0 flex-col pl-2 text-sm"
	class:hidden={!open}
	aria-label="AI assistant"
>
	<div class="flex h-11 shrink-0 items-center justify-between pr-1.5 pl-3">
		<h2 class="flex items-center gap-2 text-sm font-medium">
			<SparklesIcon class="size-4 text-signal-text" />AI assistant
		</h2>
		<div class="flex items-center gap-0.5">
			<Button
				size="icon-sm"
				variant={settingsOpen ? 'secondary' : 'ghost'}
				aria-label="Settings"
				title="Settings"
				aria-pressed={settingsOpen}
				onclick={() => (settingsOpen = !settingsOpen)}><Settings2Icon /></Button
			>
			<Button
				size="icon-sm"
				variant="ghost"
				aria-label="New chat"
				title="New chat"
				disabled={!ai.messages.length}
				onclick={() => ai.reset()}><SquarePenIcon /></Button
			>
			<Button size="icon-sm" variant="ghost" aria-label="Close AI assistant" onclick={onClose}
				><XIcon /></Button
			>
		</div>
	</div>
	{#if settingsOpen}
		<form
			class="mx-2 mb-2 flex max-h-[55%] shrink-0 flex-col gap-3 overflow-y-auto rounded-xl border bg-background p-3 shadow-panel"
			onsubmit={(e) => {
				e.preventDefault();
				void saveSettings();
			}}
		>
			<div class="flex flex-col gap-1.5">
				<label for="ai-provider" class="text-[13px] font-medium text-muted-foreground"
					>Provider</label
				>
				<select
					id="ai-provider"
					class="h-8 rounded-lg border border-input bg-background px-2 text-sm disabled:cursor-not-allowed disabled:opacity-55"
					value={config.provider}
					onchange={changeProvider}
					disabled={saving}
				>
					<option value="ollama">Ollama</option>
					<option value="openaiResponses">OpenAI Responses API</option>
					<option value="openaiCompatible">OpenAI-compatible</option>
				</select>
			</div>
			<div class="flex flex-col gap-1.5">
				<label for="ai-base-url" class="text-[13px] font-medium text-muted-foreground"
					>Base URL</label
				>
				<Input id="ai-base-url" bind:value={config.baseUrl} required disabled={saving} />
			</div>
			<div class="flex flex-col gap-1.5">
				<label for="ai-model" class="text-[13px] font-medium text-muted-foreground">Model</label>
				<Input
					id="ai-model"
					bind:value={config.model}
					placeholder="Enter a model name"
					required
					disabled={saving}
				/>
			</div>
			<div class="flex flex-col gap-1.5">
				<label for="ai-auth-mode" class="text-[13px] font-medium text-muted-foreground"
					>Authentication</label
				>
				<select
					id="ai-auth-mode"
					class="h-8 rounded-lg border border-input bg-background px-2 text-sm disabled:cursor-not-allowed disabled:opacity-55"
					bind:value={config.authMode}
					disabled={saving}
				>
					<option value="none">None</option>
					<option value="bearer">Bearer token (API key)</option>
					<option value="basic">Basic Auth</option>
				</select>
			</div>
			{#if config.authMode === 'bearer'}
				<div class="flex flex-col gap-1.5">
					<label for="ai-api-key" class="text-[13px] font-medium text-muted-foreground"
						>API key</label
					>
					<Input
						id="ai-api-key"
						type="password"
						autocomplete="off"
						bind:value={apiKey}
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
			{#if settingsError}<p role="alert" class="text-[13px] text-bad">{settingsError}</p>{/if}
			<div>
				<Button type="submit" size="sm" disabled={saving}
					>{saving ? 'Saving…' : 'Save settings'}</Button
				>
			</div>
		</form>
	{/if}
	<div class="flex shrink-0 items-center gap-1.5 px-3 pb-2 text-xs">
		<button
			type="button"
			class="measured flex h-6 min-w-0 items-center rounded-md border px-2 text-ink-2 transition-[background-color,color] duration-(--dur-micro) hover:bg-hover hover:text-foreground"
			title="{ai.settings.config.baseUrl}, click to change"
			onclick={() => (settingsOpen = true)}
		>
			<span class="truncate">{ai.settings.config.model || 'No model'}</span>
		</button>
		{#if context}
			<span
				class="flex h-6 min-w-0 items-center gap-1.5 truncate rounded-md border px-2 text-muted-foreground"
				title="Signed in as {context.user}"
			>
				<DatabaseIcon class="size-3 shrink-0" />
				<span class="truncate">{context.name} / {context.database}</span>
			</span>
		{/if}
	</div>
	<div
		class="relative mx-2 flex min-h-0 flex-1 flex-col rounded-xl border bg-background shadow-panel"
	>
		<div
			bind:this={log}
			onscroll={onLogScroll}
			class="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-3"
			role="log"
			aria-label="Chat messages"
			aria-live="polite"
		>
			{#if !context}
				<div class="m-auto flex max-w-64 flex-col items-center gap-2 text-center">
					<DatabaseIcon class="size-5 text-muted-foreground" />
					<p class="text-ink-2">Connect to a database to start chatting.</p>
				</div>
			{:else if !ai.messages.length}
				<div class="m-auto flex w-full max-w-72 flex-col gap-4">
					<div class="flex flex-col gap-1">
						<p class="text-base font-medium">Ask about {context.database}</p>
						<p class="text-[13px] text-muted-foreground">
							Your prompts and the schema go to the configured endpoint. The chat stays in memory.
						</p>
					</div>
					<div class="flex flex-col gap-1.5">
						{#each SUGGESTIONS as suggestion (suggestion)}
							<button
								type="button"
								class="rounded-lg border px-3 py-2 text-left text-[13px] text-ink-2 transition-[background-color,color,border-color] duration-(--dur-micro) hover:border-rule-2 hover:bg-hover hover:text-foreground disabled:pointer-events-none disabled:opacity-55"
								disabled={!canSend}
								onclick={() => {
									draft = suggestion;
									send();
								}}>{suggestion}</button
							>
						{/each}
					</div>
				</div>
			{/if}
			{#each ai.messages as message, index (index)}
				{#if message.role === 'user'}
					<p
						class="ml-8 self-end rounded-2xl rounded-br-md bg-hover px-3 py-2 break-words whitespace-pre-wrap text-foreground"
					>
						{message.content}
					</p>
				{:else}
					<div class="flex flex-col gap-2">
						<Markdown
							text={message.content}
							canOpenSql={!!ai.context}
							onOpenSql={(sql) => {
								const pinned = ai.context;
								if (pinned) tabs.openQuery(pinned.connectionId, pinned.database, sql);
							}}
						/>
						{#if message.tools?.length}
							<details class="text-xs text-muted-foreground">
								<summary class="w-fit cursor-pointer rounded-sm hover:text-foreground"
									>Tools used (<span class="measured">{message.tools.length}</span>)</summary
								>
								<ul class="mt-1.5 flex flex-col gap-1 border-l border-rule-2 pl-3">
									{#each message.tools as tool, toolIndex (toolIndex)}
										<li class="break-words">
											<span class="measured text-ink-2">{tool.name}</span>
											{tool.summary}
										</li>
									{/each}
								</ul>
							</details>
						{/if}
					</div>
				{/if}
			{/each}
			{#if ai.busy}<p role="status" class="flex items-center gap-2 text-muted-foreground">
					<LoaderIcon class="size-3.5 animate-spin text-cool" />Thinking…
				</p>{/if}
		</div>
		{#if !atBottom && ai.messages.length}
			<Button
				size="xs"
				variant="outline"
				class="absolute bottom-3 left-1/2 -translate-x-1/2 bg-background"
				onclick={() => scrollToEnd(true)}
			>
				<ArrowDownIcon /> Jump to latest
			</Button>
		{/if}
	</div>
	{#if ai.error}<p
			role="alert"
			class="mx-2 mt-2 rounded-lg bg-bad-soft px-3 py-2 text-xs text-ink-2"
		>
			{ai.error}
		</p>{/if}
	<form
		class="flex shrink-0 flex-col gap-2 p-2"
		onsubmit={(e) => {
			e.preventDefault();
			send();
		}}
	>
		<div
			class="flex flex-col rounded-xl border bg-background shadow-panel transition-[border-color] duration-(--dur-micro) focus-within:border-ring"
		>
			<label for="ai-prompt" class="sr-only">Message</label>
			<textarea
				id="ai-prompt"
				data-composer-input
				class="[field-sizing:content] max-h-48 min-h-11 w-full resize-none bg-transparent px-3 pt-2.5 text-sm placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-55"
				rows="1"
				bind:value={draft}
				placeholder={context ? 'Ask about this database…' : 'Connect to a database first'}
				disabled={!inputEnabled}
				onkeydown={(e) => {
					if (isSendKey(e)) {
						e.preventDefault();
						send();
					}
				}}></textarea>
			<div class="flex items-center justify-between gap-2 py-1.5 pr-1.5 pl-2">
				<label
					class="flex min-w-0 items-center gap-2 rounded-md px-1 py-0.5 text-xs text-muted-foreground"
					title="Lets the assistant run read-only queries and see sample rows. Changing this clears the chat."
				>
					<Checkbox
						checked={ai.allowData}
						onCheckedChange={(checked) => ai.setAllowData(checked)}
					/>
					<span class="truncate">Read-only queries</span>
				</label>
				{#if ai.busy}
					<Button
						type="button"
						size="icon-sm"
						variant="secondary"
						class="rounded-full"
						aria-label="Stop"
						title="Stop"
						onclick={() => ai.cancel()}><SquareIcon class="size-3 fill-current" /></Button
					>
				{:else}
					<Button
						type="submit"
						size="icon-sm"
						class="rounded-full"
						aria-label="Send"
						title="Send (↵), new line with ⇧↵"
						disabled={!canSend || !draft.trim()}><ArrowUpIcon /></Button
					>
				{/if}
			</div>
		</div>
	</form>
</aside>
