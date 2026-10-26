<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage, type QueryOutput } from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import ResultGrid from './ResultGrid.svelte';

	interface Props {
		connectionId: string;
		database: string;
	}
	let { connectionId, database }: Props = $props();
	const tabId = `sessions-${crypto.randomUUID()}`;
	let output = $state<QueryOutput | null>(null);
	let error = $state('');
	let loading = $state(false);
	let alive = true;

	async function refresh() {
		if (loading) return;
		loading = true;
		error = '';
		try {
			const result = await api.runQuery(
				tabId,
				connectionId,
				database,
				`SELECT pid, datname AS database, usename AS username, application_name, client_addr,
				 state, wait_event_type, wait_event, query_start, query
				 FROM pg_catalog.pg_stat_activity ORDER BY query_start DESC NULLS LAST`,
				10000
			);
			if (alive) output = result;
		} catch (e) {
			if (alive) error = errorMessage(e);
		} finally {
			if (alive) loading = false;
			else void api.closeTab(tabId);
		}
	}

	onMount(() => {
		void refresh();
		return () => {
			alive = false;
			void api.closeTab(tabId);
		};
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex h-11 shrink-0 items-center gap-3 border-b px-3">
		<span class="text-sm font-medium">Sessions</span>
		<span class="text-xs text-muted-foreground">Visibility depends on PostgreSQL permissions</span>
		<Button class="ml-auto" size="sm" variant="ghost" disabled={loading} onclick={refresh}
			>Refresh</Button
		>
	</div>
	{#if error}<p class="p-4 text-sm text-bad" role="alert">{error}</p>
	{:else if output?.results[0]}
		{@const result = output.results[0]}
		<div class="min-h-0 flex-1">
			<ResultGrid columns={result.columns} rows={result.rows} source={null} />
		</div>
	{:else}<p class="p-4 text-sm text-muted-foreground">Loading sessions…</p>{/if}
</div>
