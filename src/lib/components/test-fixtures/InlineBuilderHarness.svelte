<script lang="ts">
	import QueryBuilder from '../builder/QueryBuilder.svelte';
	import { importViewQuery } from '#lib/view-builder.js';
	import { buildSql } from '#lib/query-builder.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	let { onrun, onopen }: { onrun: (sql: string) => void; onopen: (sql: string) => void } = $props();
	const tables = [{ schema: 'public', name: 'customers', columns: ['id'] }];
	let model = $state(
		importViewQuery('SELECT id FROM public.customers WHERE id > 1;', tables, tables)!
	);
	let draft = $state<string | null>(null);
</script>

<Tooltip.Provider>
	<QueryBuilder
		{model}
		bind:draft
		connectionId="c"
		database="db"
		{tables}
		running={false}
		{onrun}
		{onopen}
	/>
	<output aria-label="Built query">{buildSql(model).sql}</output>
</Tooltip.Provider>
