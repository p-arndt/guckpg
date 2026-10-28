<script lang="ts">
	import { shortcut } from '#lib/shortcut.js';
	import { onMount, tick } from 'svelte';
	import PlayIcon from '@lucide/svelte/icons/play';
	import ListVideoIcon from '@lucide/svelte/icons/list-video';
	import SquareIcon from '@lucide/svelte/icons/square';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import ListFilterIcon from '@lucide/svelte/icons/list-filter';
	import SaveIcon from '@lucide/svelte/icons/save';
	import InfoIcon from '@lucide/svelte/icons/info';
	import { confirmation } from '#lib/state/confirm.svelte.js';
	import { replaceViewSql, viewEditorContent } from '#lib/view-builder.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import * as Resizable from '#lib/components/ui/resizable/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import type { CompletionTable } from '#lib/api.js';
	import { statementAt } from '#lib/sql-text.js';
	import { catalog } from '#lib/state/catalog.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import type { QueryTab } from '#lib/state/tabs.svelte.js';
	import { buildSql, emptyModel } from '#lib/query-builder.js';
	import SqlEditor from './SqlEditor.svelte';
	import QueryBuilder from './builder/QueryBuilder.svelte';
	import ResultsPanel from './ResultsPanel.svelte';
	import ExplainPlan from './ExplainPlan.svelte';
	import SaveViewDialog from './SaveViewDialog.svelte';

	interface Props {
		tab: QueryTab;
	}

	let { tab }: Props = $props();

	let saveViewOpen = $state(false);
	let editor = $state<SqlEditor | null>(null);
	let databases = $state<string[]>([]);
	let completion = $state<CompletionTable[]>([]);
	let editorPane = $state<ReturnType<typeof Resizable.Pane> | null>(null);
	/** Editor height before the builder took more room, restored when it closes. */
	let sizeBeforeBuilder: number | null = null;

	const builder = $derived(tab.builder?.database === tab.database ? tab.builder.model : null);

	function makeRoomForBuilder() {
		const size = editorPane?.getSize() ?? 100;
		if (size < 62) {
			sizeBeforeBuilder = size;
			editorPane?.resize(62);
		}
	}

	onMount(async () => {
		await tick();
		if (tab.builderOpen) makeRoomForBuilder();
	});

	async function toggleBuilder() {
		if (!tab.builderOpen && tab.viewSqlSnapshot !== null && tab.sql !== tab.viewSqlSnapshot) {
			const confirmed = await confirmation.ask({
				title: 'Discard SQL changes?',
				description:
					'The visual builder cannot include the SQL edits you made. Discard them to return to the visual editor.',
				confirmLabel: 'Discard SQL changes',
				destructive: true
			});
			if (!confirmed) return;
			tab.sql = tab.viewSqlSnapshot;
		}
		tab.builderOpen = !tab.builderOpen;
		if (tab.builderOpen) {
			if (!builder) tab.builder = { database: tab.database, model: emptyModel() };
			makeRoomForBuilder();
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
	const editingView = $derived(tab.editingView?.database === tab.database ? tab.editingView : null);

	const rowLimitLabel = (n: string) => Number(n).toLocaleString('en-US');
	const quietTrigger =
		'h-7 gap-1.5 border-transparent bg-transparent text-[13px] shadow-none hover:bg-hover dark:bg-transparent dark:hover:bg-hover data-[state=open]:bg-hover';

	// The cleanups drop answers for a connection or database the tab has moved away from.
	$effect(() => {
		const { connectionId, database } = tab;
		let current = true;
		catalog.databases(connectionId).then(
			(list) => {
				if (current) databases = list;
			},
			() => {
				if (current) databases = [database];
			}
		);
		return () => {
			current = false;
		};
	});

	$effect(() => {
		const { connectionId, database } = tab;
		let current = true;
		completion = [];
		catalog.completionTables(connectionId, database).then(
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

	async function run(text: string, from: number, inEditor = true) {
		editor?.markError(null);
		await tab.execute(text, from, inEditor);
		editor?.markError(tab.errorOffset);
	}

	function execute(all: boolean) {
		if (tab.running || !editor) return;
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
		void run(text, from);
	}

	function explain() {
		if (tab.running || !editor) return;
		const text =
			tab.builderOpen && builder
				? buildSql(builder).sql
				: (editor.selection()?.text ?? statementAt(tab.sql, editor.cursor())?.text);
		if (text) void tab.explain(text);
	}

	function runBuilt(sql: string) {
		void run(sql, 0, false);
	}

	function openInEditor(sql: string) {
		const text = editingView
			? replaceViewSql(editingView.schema, editingView.name, editingView.details, sql)
			: sql;
		const { prefix, sql: editorSql } = viewEditorContent(
			tab.sql,
			editingView ? tab.viewSqlSnapshot : null,
			editingView ? tab.viewSqlPrefix : null,
			text
		);
		tab.sql = editorSql;
		if (editingView) {
			tab.viewSqlPrefix = prefix;
			tab.viewSqlSnapshot = tab.sql;
			tab.viewNotice =
				'You are editing the view as SQL. Run the statement to apply changes; SQL edits do not update the visual builder.';
		}
		void toggleBuilder();
	}

	async function rerun() {
		editor?.markError(null);
		await tab.rerun();
		editor?.markError(tab.errorOffset);
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex h-11 shrink-0 items-center gap-1 px-2">
		{#if tab.creatingView || editingView}
			<span
				class="max-w-48 truncate pr-2 text-[13px] font-medium"
				title={editingView
					? `${editingView.schema}.${editingView.name}`
					: `New view in ${tab.viewSchema}`}
			>
				{editingView
					? `View · ${editingView.schema}.${editingView.name}`
					: `New view · ${tab.viewSchema}`}
			</span>
		{/if}
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
		<Select.Root
			type="single"
			value={String(tab.maxRows)}
			onValueChange={(v) => (tab.maxRows = Number(v))}
		>
			<Select.Trigger size="sm" class={quietTrigger} aria-label="Row limit">
				<span class="text-muted-foreground">Limit</span>
				<span class="measured">{rowLimitLabel(String(tab.maxRows))}</span>
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
				disabled={!!tab.viewNotice && !builder}
				onclick={toggleBuilder}
			>
				<ListFilterIcon />
				{tab.creatingView || editingView ? 'Visual editor' : 'Query builder'}
			</Button>
			{#if tab.builderOpen && builder}
				{@const built = buildSql(builder)}
				<Button
					size="sm"
					variant="outline"
					disabled={!built.sql || tab.running || (connection?.readOnly ?? true)}
					title={!built.sql
						? 'Add a table and complete the query first'
						: connection?.readOnly
							? 'Read-only connection'
							: 'Review the view SQL'}
					onclick={() => (saveViewOpen = true)}
				>
					<SaveIcon />
					{editingView ? 'Save view' : tab.creatingView ? 'Create view' : 'Save as view'}
				</Button>
			{/if}
			<Button size="sm" variant="ghost" disabled={tab.running} onclick={explain}>Explain</Button>
			{#if tab.running}
				<Button size="sm" variant="ghost" class="text-bad" onclick={() => tab.cancel()}>
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
				<Tooltip.Content
					>Run the whole script <kbd class="key">{shortcut('↵', true)}</kbd></Tooltip.Content
				>
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
									if (sql) runBuilt(sql);
								} else void execute(false);
							}}
						>
							<PlayIcon class="fill-current" />
							{tab.builderOpen ? 'Preview' : 'Run'}
							<kbd
								class="ml-1 rounded-[4px] bg-primary-foreground/15 px-1 text-[11px] leading-4 font-normal"
								>{shortcut('↵')}</kbd
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

	{#if tab.viewNotice && !tab.builderOpen}
		<div
			class="flex shrink-0 items-start gap-2 border-t border-b bg-signal-soft px-3 py-2 text-xs text-ink-2"
			role="status"
		>
			<InfoIcon class="mt-0.5 size-4 shrink-0 text-signal-text" /><span>{tab.viewNotice}</span>
		</div>
	{/if}
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
			{#if tab.plan !== null}
				<ExplainPlan value={tab.plan} />
			{:else}
				<ResultsPanel
					running={tab.running}
					output={tab.output}
					error={tab.error}
					sql={tab.resultSql}
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
			{/if}
		</Resizable.Pane>
	</Resizable.PaneGroup>
</div>

{#if builder && saveViewOpen}
	{@const built = buildSql(builder)}
	<SaveViewDialog
		bind:open={saveViewOpen}
		connectionId={tab.connectionId}
		database={tab.database}
		query={built.sql}
		problems={built.problems}
		defaultSchema={tab.viewSchema}
		{editingView}
		onCreated={(schema, name, details) => {
			tab.creatingView = false;
			if (details) {
				tab.title = `View: ${name}`;
				tab.editingView = { database: tab.database, schema, name, details };
			}
		}}
	/>
{/if}
