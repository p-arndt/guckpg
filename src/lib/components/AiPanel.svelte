<script lang="ts">
	import { tick, untrack } from 'svelte';
	import {
		aiApi,
		defaultAiConfig,
		splitApprovals,
		currentActivityLabel,
		completedActivityCount,
		toolOutcome,
		approvalOutcome,
		thinkingPreview,
		type AiProvider,
		type AiActivity,
		type ToolOutcome,
		type ToolState
	} from '#lib/ai.js';
	import { fade, slide } from 'svelte/transition';
	import { prefersReducedMotion } from 'svelte/motion';
	import { errorMessage } from '#lib/api.js';
	import { highlightSql } from '#lib/highlight.js';
	import { ai, type AiAccess } from '#lib/state/ai.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import XIcon from '@lucide/svelte/icons/x';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import CheckIcon from '@lucide/svelte/icons/check';
	import BanIcon from '@lucide/svelte/icons/ban';
	import BrainIcon from '@lucide/svelte/icons/brain';
	import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
	import CircleXIcon from '@lucide/svelte/icons/circle-x';
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import EyeIcon from '@lucide/svelte/icons/eye';
	import ListTreeIcon from '@lucide/svelte/icons/list-tree';
	import PencilLineIcon from '@lucide/svelte/icons/pencil-line';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import Settings2Icon from '@lucide/svelte/icons/settings-2';
	import SparklesIcon from '@lucide/svelte/icons/sparkles';
	import SquareIcon from '@lucide/svelte/icons/square';
	import SquarePenIcon from '@lucide/svelte/icons/square-pen';
	import { followChatEnd, nextFollowing, stopsFollowing } from '#lib/chat-scroll.js';
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
	let models = $state<string[]>([]);
	let modelsLoaded = $state(false);
	let loadingModels = $state(false);
	let modelRequest = 0;
	async function loadModels() {
		const request = ++modelRequest;
		loadingModels = true;
		modelsLoaded = false;
		models = [];
		settingsError = '';
		let timer: ReturnType<typeof setTimeout> | undefined;
		try {
			const timeout = new Promise<never>((_, reject) => {
				timer = setTimeout(
					() =>
						reject(
							new Error('Loading models timed out. Check the URL, connection, and authentication.')
						),
					15000
				);
			});
			const result = await Promise.race([
				aiApi.models(
					{ ...config, baseUrl: config.baseUrl.trim() },
					config.authMode === 'bearer' && apiKey ? apiKey : undefined,
					config.authMode === 'basic' && (basicUsername || basicPassword)
						? { username: basicUsername, password: basicPassword }
						: undefined
				),
				timeout
			]);
			if (request === modelRequest) {
				models = result;
				modelsLoaded = true;
			}
		} catch (e) {
			if (request === modelRequest) settingsError = errorMessage(e);
		} finally {
			clearTimeout(timer);
			if (request === modelRequest) loadingModels = false;
		}
	}
	function invalidateModels() {
		modelRequest++;
		models = [];
		modelsLoaded = false;
		loadingModels = false;
		settingsError = '';
	}
	function closeSettings() {
		invalidateModels();
		config = { ...ai.settings.config };
		apiKey = '';
		basicUsername = '';
		basicPassword = '';
		clearKey = false;
		clearBasic = false;
		settingsOpen = false;
	}
	function openSettings() {
		settingsOpen = true;
		if (config.provider === 'ollama') void loadModels();
	}
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
			user: connection.user,
			readOnly: connection.readOnly
		};
	});

	async function loadSettings() {
		await ai.load();
		config = { ...ai.settings.config };
		if (!config.model) openSettings();
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
		invalidateModels();
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

	let elapsedSeconds = $state(0);
	$effect(() => {
		if (!ai.busy) return;
		const started = performance.now();
		elapsedSeconds = 0;
		const timer = setInterval(() => {
			elapsedSeconds = Math.floor((performance.now() - started) / 1000);
		}, 1000);
		return () => clearInterval(timer);
	});

	const inputEnabled = $derived(!!context && !ai.busy && !saving && !settingsOpen && ai.loaded);
	const canSend = $derived(inputEnabled);
	const accessLevels = [
		{
			value: 'schema',
			label: 'Schema only',
			hint: 'Sees tables and columns, never row data.',
			icon: ListTreeIcon,
			tone: 'text-muted-foreground'
		},
		{
			value: 'read',
			label: 'Read data',
			hint: 'Can run read-only queries.',
			icon: EyeIcon,
			tone: 'text-cool'
		},
		{
			value: 'write',
			label: 'Read & write',
			hint: 'Can also propose changes; each one needs your approval.',
			icon: PencilLineIcon,
			tone: 'text-signal-text'
		}
	] as const satisfies readonly { value: AiAccess; [key: string]: unknown }[];
	const currentAccess = $derived(
		accessLevels.find((level) => level.value === ai.access) ?? accessLevels[0]
	);

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

	let lastScrollTop = 0;

	function onLogScroll() {
		if (!log) return;
		atBottom = nextFollowing({ scrollTop: lastScrollTop, following: atBottom }, log);
		lastScrollTop = log.scrollTop;
	}

	// Listened to passively: the log only observes upward intent, it never handles the input.
	$effect(() => {
		const target = log;
		if (!target) return;
		const onIntent = (event: WheelEvent | KeyboardEvent) => {
			if (stopsFollowing(event)) atBottom = false;
		};
		target.addEventListener('wheel', onIntent, { passive: true });
		target.addEventListener('keydown', onIntent);
		return () => {
			target.removeEventListener('wheel', onIntent);
			target.removeEventListener('keydown', onIntent);
		};
	});

	function scrollToEnd(smooth: boolean) {
		if (!log) return;
		const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
		log.scrollTo({ top: log.scrollHeight, behavior: smooth && !reduce ? 'smooth' : 'auto' });
		atBottom = true;
	}

	$effect(() => {
		void ai.messages.length;
		void ai.busy;
		void ai.streamedText;
		const lastActivity = ai.activity.at(-1);
		if (lastActivity?.type === 'thinking') void lastActivity.text;
		if (lastActivity?.type === 'tool') {
			void lastActivity.summary;
			void lastActivity.approval;
		}
		const target = log;
		if (target && untrack(() => atBottom)) {
			void tick().then(() => followChatEnd(target, untrack(() => atBottom) && target === log));
		}
	});

	// Reopening the panel lands on the latest message.
	$effect(() => {
		if (!open) return;
		lastScrollTop = 0;
		void tick().then(() => untrack(() => scrollToEnd(false)));
	});
</script>

{#snippet sqlPreview(sql: string)}
	<div class="overflow-hidden rounded-lg border border-rule-2 bg-background">
		<div
			class="flex items-center justify-between border-b border-rule-2 px-3 py-1.5 text-[11px] font-medium tracking-wide text-muted-foreground"
		>
			<span>SQL</span><span>PostgreSQL</span>
		</div>
		<pre
			class="measured max-h-56 overflow-auto px-3 py-2.5 text-xs leading-relaxed break-words whitespace-pre-wrap text-foreground"><code
				>{#each highlightSql(sql) as token, index (index)}{#if token.classes}<span
							class={token.classes}>{token.text}</span
						>{:else}{token.text}{/if}{/each}</code
			></pre>
	</div>
{/snippet}

{#snippet statusIcon(state: ToolState)}
	<span class="flex size-4 shrink-0 items-center justify-center" aria-hidden="true">
		{#if state === 'running'}
			<LoaderIcon class="size-3.5 animate-spin text-cool" />
		{:else if state === 'done'}
			<CheckIcon class="size-3.5 text-cool" />
		{:else if state === 'blocked'}
			<BanIcon class="size-3.5 text-muted-foreground" />
		{:else}
			<XIcon class="size-3.5 text-muted-foreground" />
		{/if}
	</span>
{/snippet}

{#snippet expandChevron(group: 'thought' | 'tool' | 'approval')}
	<ChevronRightIcon
		aria-hidden="true"
		class={[
			'size-3 shrink-0 opacity-60 transition-transform duration-(--dur-micro) motion-reduce:transition-none',
			group === 'thought' && 'group-open/thought:rotate-90',
			group === 'tool' && 'group-open/tool:rotate-90',
			group === 'approval' && 'group-open/approval:rotate-90'
		]}
	/>
{/snippet}

{#snippet toolLine(name: string, outcome: ToolOutcome)}
	{@render statusIcon(outcome.state)}
	<span class="min-w-0 truncate text-ink-2">{name.replaceAll('_', ' ')}</span>
	{#if outcome.label}
		<span class="min-w-0 shrink truncate text-muted-foreground">{outcome.label}</span>
	{/if}
	<span class="sr-only"
		>{outcome.state === 'running' ? 'running' : outcome.state === 'done' ? 'completed' : ''}</span
	>
{/snippet}

{#snippet toolDetail(detail: string, failed: boolean)}
	{#if failed}
		<p
			class="rounded-md bg-bad-soft px-2.5 py-1.5 leading-relaxed break-words whitespace-pre-wrap text-bad"
		>
			{detail}
		</p>
	{:else}
		<pre
			class="measured border-rule max-h-48 overflow-auto rounded-md border bg-background px-2.5 py-1.5 text-[11px] leading-relaxed break-words whitespace-pre-wrap text-ink-2">{detail}</pre>
	{/if}
{/snippet}

{#snippet activityList(items: AiActivity[])}
	{@const row = 'flex min-h-6 w-full min-w-0 items-center gap-2 rounded-md px-1.5 py-0.5 text-left'}
	{@const expandableRow = `${row} cursor-pointer list-none hover:bg-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring [&::-webkit-details-marker]:hidden`}
	<div class="flex min-w-0 flex-col py-0.5 text-xs text-muted-foreground">
		{#each items as item, index (index)}
			{#if item.type === 'thinking'}
				{@const preview = thinkingPreview(item.text)}
				{#if preview}
					<details class="group/thought min-w-0">
						<summary class={expandableRow} title="Show reasoning">
							<span class="flex size-4 shrink-0 items-center justify-center" aria-hidden="true">
								<BrainIcon class="size-3.5" />
							</span>
							<span class="min-w-0 truncate italic">{preview}</span>
							{@render expandChevron('thought')}
						</summary>
						<p
							class="mt-0.5 mb-1 ml-6 max-h-64 overflow-auto pr-2 leading-relaxed break-words whitespace-pre-wrap"
						>
							{item.text.trim()}
						</p>
					</details>
				{/if}
			{:else}
				{@const outcome = toolOutcome(item.summary)}
				{#if item.detail || outcome.details}
					<details class="group/tool min-w-0">
						<summary class={expandableRow}>
							{@render toolLine(item.name, outcome)}
							{@render expandChevron('tool')}
						</summary>
						<div class="mt-0.5 mb-1 ml-6 flex min-w-0 flex-col gap-1.5">
							{#if item.detail}{@render toolDetail(
									item.detail,
									outcome.state === 'blocked' || outcome.state === 'failed'
								)}{/if}
							{#if outcome.details}{@render toolDetail(outcome.details, false)}{/if}
						</div>
					</details>
				{:else}
					<div class={row}>{@render toolLine(item.name, outcome)}</div>
				{/if}
			{/if}
		{/each}
	</div>
{/snippet}

{#snippet approvalList(items: AiActivity[])}
	{@const duration = prefersReducedMotion.current ? 0 : 180}
	<div class="flex min-w-0 flex-col gap-2">
		{#each items as item, index (index)}
			{#if item.type === 'tool' && item.approval}
				{#if item.summary === null}
					<section
						class="overflow-hidden rounded-xl border border-cool/40 bg-background text-xs shadow-panel"
						aria-label="Write approval"
						out:slide={{ duration }}
					>
						<div class="flex items-start gap-2.5 border-b border-rule-2 px-3 py-3">
							<ShieldCheckIcon class="mt-0.5 size-4 shrink-0 text-cool" />
							<div class="min-w-0">
								<p class="font-medium text-foreground">Review database change</p>
								<p class="mt-0.5 break-words text-muted-foreground">
									{context?.name} / {context?.database}
								</p>
							</div>
						</div>
						<div class="flex flex-col gap-3 p-3">
							{@render sqlPreview(item.approval.sql)}
							<p class="leading-relaxed text-muted-foreground">
								Review the entire statement before approving. It may delete data or trigger
								additional changes.
							</p>
							{#if item.approval.decision === undefined}
								<div class="flex flex-wrap justify-end gap-2">
									<Button
										type="button"
										size="sm"
										variant="outline"
										onclick={() => ai.decideWrite(item.approval!.id, false)}>Reject</Button
									>
									<Button
										type="button"
										size="sm"
										onclick={() => ai.decideWrite(item.approval!.id, true)}
										>Approve and execute</Button
									>
								</div>
							{:else}
								<p role="status" class="flex items-center gap-2 text-ink-2">
									<LoaderIcon class="size-3 animate-spin" />{item.approval.decision
										? 'Executing approved statement…'
										: 'Rejecting…'}
								</p>
							{/if}
						</div>
					</section>
				{:else}
					{@const outcome = approvalOutcome(item.summary)}
					<details class="group/approval min-w-0 text-xs" in:fade={{ duration }}>
						<summary
							class={[
								'flex w-fit max-w-full cursor-pointer list-none items-center gap-2 rounded-md px-2 py-1 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring [&::-webkit-details-marker]:hidden',
								outcome.tone === 'ok' && 'bg-ok-soft text-ok',
								outcome.tone === 'bad' && 'bg-bad-soft text-bad',
								outcome.tone === 'muted' && 'text-muted-foreground hover:bg-hover'
							]}
						>
							<span class="flex size-4 shrink-0 items-center justify-center" aria-hidden="true">
								{#if outcome.tone === 'ok'}
									<CircleCheckIcon class="size-3.5" />
								{:else if outcome.tone === 'bad'}
									<CircleXIcon class="size-3.5" />
								{:else}
									<BanIcon class="size-3.5" />
								{/if}
							</span>
							<span class="font-medium">{outcome.title}</span>
							{#if outcome.detail}
								<span class="min-w-0 truncate tabular-nums opacity-80">· {outcome.detail}</span>
							{/if}
							{@render expandChevron('approval')}
						</summary>
						<div class="mt-1.5 flex min-w-0 flex-col gap-1.5">
							{#if item.detail}{@render toolDetail(item.detail, outcome.tone !== 'ok')}{/if}
							{@render sqlPreview(item.approval.sql)}
						</div>
					</details>
				{/if}
			{/if}
		{/each}
	</div>
{/snippet}

{#snippet activityGroup(items: AiActivity[], durationMs?: number, running = false)}
	{@const groups = splitApprovals(items)}
	{@const steps = completedActivityCount(items)}
	{#if groups.history.length || running}
		<details class="group/activity min-w-0 text-xs text-muted-foreground">
			<summary
				class="flex w-fit cursor-pointer list-none items-center gap-1.5 rounded-md py-0.5 tabular-nums hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring [&::-webkit-details-marker]:hidden"
			>
				{#if running}<LoaderIcon class="size-3 animate-spin" aria-hidden="true" />{/if}
				<span class={running ? 'text-ink-2' : ''}
					>{running
						? ai.streamedText
							? 'Responding…'
							: currentActivityLabel(items)
						: 'Worked'}</span
				>
				<span aria-hidden="true" class="opacity-60">·</span>
				<span class="whitespace-nowrap">{steps} {steps === 1 ? 'step' : 'steps'}</span>
				<span aria-hidden="true" class="opacity-60">·</span>
				<span class="whitespace-nowrap"
					>{running ? elapsedSeconds : Math.max(1, Math.round((durationMs ?? 0) / 1000))}s</span
				>
				<ChevronRightIcon
					aria-hidden="true"
					class="size-3 opacity-60 transition-transform duration-(--dur-micro) group-open/activity:rotate-90 motion-reduce:transition-none"
				/>
			</summary>
			<div class="mt-0.5 -ml-1.5">
				{@render activityList(groups.history)}
			</div>
		</details>
	{/if}
	{#if groups.approvals.length}{@render approvalList(groups.approvals)}{/if}
{/snippet}

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
				onclick={() => (settingsOpen ? closeSettings() : openSettings())}><Settings2Icon /></Button
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
			class="mx-2 mb-2 flex min-h-0 flex-1 flex-col rounded-xl border bg-background shadow-panel"
			onsubmit={(e) => {
				e.preventDefault();
				void saveSettings();
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
					onclick={closeSettings}><XIcon /></Button
				>
			</div>
			<div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
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
					<Input
						id="ai-base-url"
						bind:value={config.baseUrl}
						oninput={invalidateModels}
						required
						disabled={saving}
					/>
				</div>
				<div class="flex flex-col gap-1.5">
					<label for="ai-model" class="text-[13px] font-medium text-muted-foreground">Model</label>
					{#if config.provider === 'ollama'}
						<div class="flex gap-2">
							<select
								aria-label="Available Ollama models"
								class="h-8 min-w-0 flex-1 rounded-lg border border-input bg-background px-2 text-sm"
								value={models.includes(config.model) ? config.model : ''}
								onchange={(e) => {
									if (e.currentTarget.value) config.model = e.currentTarget.value;
								}}
								disabled={!models.length || saving}
							>
								<option value=""
									>{loadingModels
										? 'Loading models…'
										: modelsLoaded && !models.length
											? 'No models found'
											: 'Select a model'}</option
								>
								{#each models as model (model)}<option value={model}>{model}</option>{/each}
							</select>
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
									? `${models.length} models available`
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
					<select
						id="ai-auth-mode"
						class="h-8 rounded-lg border border-input bg-background px-2 text-sm disabled:cursor-not-allowed disabled:opacity-55"
						bind:value={config.authMode}
						onchange={invalidateModels}
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
				{#if settingsError}<p
						role="alert"
						class="rounded-lg bg-bad-soft px-3 py-2 text-[13px] text-bad"
					>
						{settingsError}
					</p>{/if}
			</div>
			<div class="flex shrink-0 justify-end gap-2 border-t px-4 py-3">
				<Button type="button" size="sm" variant="ghost" onclick={closeSettings}>Cancel</Button>
				<Button type="submit" size="sm" disabled={saving}
					>{saving ? 'Saving…' : 'Save settings'}</Button
				>
			</div>
		</form>
	{:else}
		<div class="flex shrink-0 items-center gap-1.5 px-3 pb-2 text-xs">
			<button
				type="button"
				class="measured flex h-6 min-w-0 items-center rounded-md border px-2 text-ink-2 transition-[background-color,color] duration-(--dur-micro) hover:bg-hover hover:text-foreground"
				title="{ai.settings.config.baseUrl}, click to change"
				onclick={openSettings}
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
				class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3"
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
						<div class="flex flex-col gap-3">
							{#if message.activity?.length}{@render activityGroup(
									message.activity,
									message.durationMs
								)}{/if}
							<Markdown
								text={message.content}
								canOpenSql={!!ai.context}
								onOpenSql={(sql) => {
									const pinned = ai.context;
									if (pinned) tabs.openQuery(pinned.connectionId, pinned.database, sql);
								}}
							/>
						</div>
					{/if}
				{/each}
				{#if ai.busy}{@render activityGroup(ai.activity, undefined, true)}{/if}
				{#if ai.busy && ai.streamedText}<Markdown text={ai.streamedText} canOpenSql={false} />{/if}
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
				class="flex flex-col rounded-2xl border bg-background shadow-panel transition-[border-color] duration-(--dur-micro) focus-within:border-ring"
			>
				<label for="ai-prompt" class="sr-only">Message</label>
				<textarea
					id="ai-prompt"
					data-composer-input
					class="[field-sizing:content] max-h-48 min-h-12 w-full resize-none bg-transparent px-3.5 pt-3 text-sm outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-55"
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
				<div class="flex items-center justify-between gap-2 px-2 pt-1 pb-2">
					<DropdownMenu.Root>
						<DropdownMenu.Trigger disabled={ai.busy || !context}>
							{#snippet child({ props })}
								<button
									{...props}
									type="button"
									title="What the assistant may access"
									class="inline-flex h-7 min-w-0 items-center gap-1.5 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:bg-hover data-[state=open]:text-foreground"
								>
									<currentAccess.icon class="size-3.5 shrink-0 {currentAccess.tone}" />
									<span class="truncate">{currentAccess.label}</span>
									<ChevronDownIcon class="size-3 shrink-0 opacity-70" />
								</button>
							{/snippet}
						</DropdownMenu.Trigger>
						<DropdownMenu.Content side="top" align="start" class="w-72">
							<DropdownMenu.Label class="text-xs font-normal text-muted-foreground"
								>Assistant access</DropdownMenu.Label
							>
							<DropdownMenu.RadioGroup
								value={ai.access}
								onValueChange={(value) => ai.setAccess(value as AiAccess)}
							>
								{#each accessLevels as level (level.value)}
									{@const locked = level.value === 'write' && !!context?.readOnly}
									<DropdownMenu.RadioItem
										value={level.value}
										disabled={locked}
										closeOnSelect
										class="items-start py-1.5"
									>
										<level.icon class="mt-0.5 size-4 {level.tone}" />
										<span class="flex min-w-0 flex-col gap-0.5">
											<span class="font-medium">{level.label}</span>
											<span class="text-xs text-muted-foreground"
												>{locked ? 'This connection is read-only.' : level.hint}</span
											>
										</span>
									</DropdownMenu.RadioItem>
								{/each}
							</DropdownMenu.RadioGroup>
							<DropdownMenu.Separator />
							<p class="px-1.5 py-1 text-xs text-muted-foreground">
								Changing access starts a new chat.
							</p>
						</DropdownMenu.Content>
					</DropdownMenu.Root>
					{#if ai.busy}
						<Button
							type="button"
							size="icon-sm"
							variant="secondary"
							class="shrink-0 rounded-lg"
							aria-label="Stop"
							title="Stop"
							onclick={() => ai.cancel()}><SquareIcon class="size-3 fill-current" /></Button
						>
					{:else}
						<Button
							type="submit"
							size="icon-sm"
							class="shrink-0 rounded-lg disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100"
							aria-label="Send"
							title="Send (↵), new line with ⇧↵"
							disabled={!canSend || !draft.trim()}><ArrowUpIcon /></Button
						>
					{/if}
				</div>
			</div>
		</form>
	{/if}
</aside>
