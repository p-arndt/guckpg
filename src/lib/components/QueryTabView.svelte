<script lang="ts">
	import PlayIcon from '@lucide/svelte/icons/play';
	import ListVideoIcon from '@lucide/svelte/icons/list-video';
	import SquareIcon from '@lucide/svelte/icons/square';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import ListFilterIcon from '@lucide/svelte/icons/list-filter';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import * as Resizable from '#lib/components/ui/resizable/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import { api, errorMessage, isAppError, type CompletionTable } from '#lib/api.js';
	import { statementAt } from '#lib/sql-text.js';
	import { confirmDiscard } from '#lib/state/confirm.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import type { QueryTab } from '#lib/state/tabs.svelte.js';
	import { buildSql, emptyModel } from '#lib/query-builder.js';
	import SqlEditor from './SqlEditor.svelte';
	import QueryBuilder from './builder/QueryBuilder.svelte';
	import ResultsPanel from './ResultsPanel.svelte';

	interface Props {
		tab: QueryTab;
	}

	let { tab }: Props = $props();

	let editor = $state<SqlEditor | null>(null);
	let databases = $state<string[]>([]);
	let completion = $state<CompletionTable[]>([]);
	let maxRows = $state('1000');
	let editorPane = $state<ReturnType<typeof Resizable.Pane> | null>(null);
	/** Editor height before the builder took more room, restored when it closes. */
	let sizeBeforeBuilder: number | null = null;

	const builder = $derived(tab.builder?.database === tab.database ? tab.builder.model : null);

	function toggleBuilder() {
		tab.builderOpen = !tab.builderOpen;
		if (tab.builderOpen) {
			if (!builder) tab.builder = { database: tab.database, model: emptyModel() };
			const size = editorPane?.getSize() ?? 100;
			// The canvas needs more height than an editor does.
			if (size < 62) {
				sizeBeforeBuilder = size;
				editorPane?.resize(62);
			}
		} else if (sizeBeforeBuilder !== null) {
			editorPane?.resize(sizeBeforeBuilder);
			sizeBeforeBuilder = null;
		}
	}

	// A builder belongs to one database; switching starts a fresh one.
	$effect(() => {
		if (tab.builderOpen && !builder) tab.builder = { database: tab.database, model: emptyModel() };
	});

	const connection = $derived(connections.get(tab.connectionId));

	const rowLimitLabel = (n: string) => Number(n).toLocaleString('en-US');
	const quietTrigger =
		'h-7 gap-1.5 border-transparent bg-transparent text-[13px] shadow-none hover:bg-hover dark:bg-transparent dark:hover:bg-hover data-[state=open]:bg-hover';

	$effect(() => {
		const id = tab.connectionId;
		api.listDatabases(id).then(
			(list) => (databases = list),
			() => (databases = [tab.database])
		);
	});

	$effect(() => {
		const { connectionId, database } = tab;
		let current = true;
		completion = [];
		api.completionTables(connectionId, database).then(
			(tables) => {
				if (current) completion = tables;
			},
			() => {
				if (current) completion = [];
			}
		);
		return () => {
			current = false;
		};
	});

	/** Last executed text, re-run after saving grid edits. */
	let lastRun: { text: string; from: number; inEditor: boolean } | null = null;

	async function execute(all: boolean) {
		if (tab.running || !editor) return;
		if (!(await confirmDiscard(tab.dirty))) return;
		let text: string;
		let from: number;
		const selection = editor.selection();
		if (all) {
			text = tab.sql;
			from = 0;
		} else if (selection) {
			({ text, from } = selection);
		} else {
			const statement = statementAt(tab.sql, editor.cursor());
			if (!statement) return;
			({ text, from } = statement);
		}
		if (!text.trim()) return;
		await run(text, from);
	}

	async function runBuilt(sql: string) {
		if (tab.running) return;
		if (!(await confirmDiscard(tab.dirty))) return;
		await run(sql, 0, false);
	}

	function openInEditor(sql: string) {
		tab.sql = tab.sql.trim() ? `${tab.sql.trimEnd()}\n\n${sql}\n` : `${sql}\n`;
		toggleBuilder();
	}

	/** `inEditor` is false for builder SQL, whose error positions do not map onto the editor text. */
	async function run(text: string, from: number, inEditor = true) {
		if (!editor) return;
		lastRun = { text, from, inEditor };
		tab.running = true;
		tab.error = null;
		tab.ranFrom = from;
		editor.markError(null);
		try {
			tab.output = await api.runQuery(
				tab.id,
				tab.connectionId,
				tab.database,
				text,
				Number(maxRows)
			);
		} catch (e) {
			tab.output = null;
			tab.error = isAppError(e) ? e : { kind: 'message', message: errorMessage(e) };
			if (tab.error.kind === 'postgres' && tab.error.position !== null) {
				if (inEditor) editor.markError(from + tab.error.position - 1);
			}
		} finally {
			tab.running = false;
		}
	}

	function rerun() {
		if (lastRun) void run(lastRun.text, lastRun.from, lastRun.inEditor);
	}

	async function cancel() {
		try {
			await api.cancelQuery(tab.id);
		} catch (e) {
			tab.error = isAppError(e) ? e : { kind: 'message', message: errorMessage(e) };
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex h-11 shrink-0 items-center gap-1 px-2">
		<Select.Root type="single" bind:value={tab.database} disabled={tab.running}>
			<Select.Trigger size="sm" class={quietTrigger} aria-label="Database">
				<DatabaseIcon class="size-3.5 text-muted-foreground" />
				<span class="font-medium">{tab.database}</span>
			</Select.Trigger>
			<Select.Content>
				{#each databases as db (db)}
					<Select.Item value={db} label={db} />
				{/each}
			</Select.Content>
		</Select.Root>
		<Select.Root type="single" bind:value={maxRows}>
			<Select.Trigger size="sm" class={quietTrigger} aria-label="Row limit">
				<span class="text-muted-foreground">Limit</span>
				<span class="measured">{rowLimitLabel(maxRows)}</span>
			</Select.Trigger>
			<Select.Content>
				{#each ['100', '1000', '10000', '100000'] as n (n)}
					<Select.Item value={n} label={`${rowLimitLabel(n)} rows`} />
				{/each}
			</Select.Content>
		</Select.Root>

		<div class="ml-auto flex items-center gap-1.5">
			<Button
				size="sm"
				variant={tab.builderOpen ? 'secondary' : 'ghost'}
				aria-pressed={tab.builderOpen}
				onclick={toggleBuilder}
			>
				<ListFilterIcon /> Query builder
			</Button>
			{#if tab.running}
				<Button size="sm" variant="ghost" class="text-bad" onclick={cancel}>
					<SquareIcon /> Stop
				</Button>
			{/if}
			<Tooltip.Root>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button
							{...props}
							size="sm"
							variant="ghost"
							disabled={tab.running || tab.builderOpen}
							onclick={() => execute(true)}
						>
							<ListVideoIcon /> Run all
						</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Run the whole script <kbd class="key">⇧⌘↵</kbd></Tooltip.Content>
			</Tooltip.Root>
			<Tooltip.Root>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button
							{...props}
							size="sm"
							class="pr-1.5"
							disabled={tab.running}
							onclick={() => {
								if (tab.builderOpen && builder) {
									const { sql } = buildSql(builder);
									if (sql) void runBuilt(sql);
								} else void execute(false);
							}}
						>
							<PlayIcon class="fill-current" /> Run
							<kbd
								class="ml-1 rounded-[4px] bg-primary-foreground/15 px-1 text-[11px] leading-4 font-normal"
								>⌘↵</kbd
							>
						</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content
					>{tab.builderOpen
						? 'Run the built query'
						: 'Run the selection or the statement at the cursor'}</Tooltip.Content
				>
			</Tooltip.Root>
		</div>
	</div>

	<Resizable.PaneGroup direction="vertical" autoSaveId="guckpg.query" class="min-h-0 flex-1">
		<Resizable.Pane bind:this={editorPane} defaultSize={45} minSize={10}>
			<div class="h-full" class:hidden={tab.builderOpen}>
				<SqlEditor
					bind:this={editor}
					bind:value={tab.sql}
					{completion}
					onRun={() => execute(false)}
					onRunAll={() => execute(true)}
				/>
			</div>
			{#if tab.builderOpen && builder}
				<QueryBuilder
					model={builder}
					connectionId={tab.connectionId}
					database={tab.database}
					tables={completion}
					running={tab.running}
					onrun={runBuilt}
					onopen={openInEditor}
				/>
			{/if}
		</Resizable.Pane>
		<Resizable.Handle />
		<Resizable.Pane defaultSize={55} minSize={10}>
			<ResultsPanel
				running={tab.running}
				output={tab.output}
				error={tab.error}
				editTarget={connection
					? {
							connectionId: tab.connectionId,
							database: tab.database,
							readOnly: connection.readOnly
						}
					: null}
				onSaved={rerun}
				onDirtyChange={(n) => (tab.dirty = n)}
			/>
		</Resizable.Pane>
	</Resizable.PaneGroup>
</div>
