<script lang="ts">
	import { addTable, emptyModel } from '#lib/query-builder.js';
	import ClausePanel from './ClausePanel.svelte';

	let model = $state(emptyModel());
	const table = addTable(model, 'public', 'orders');
	model.columns.push({ tableId: table.id, column: 'id', aggregate: 'COUNT', alias: '' });
	const columns = [{ value: `${table.id}.id`, label: 'orders.id' }];
</script>

<ClausePanel {model} {columns} />
