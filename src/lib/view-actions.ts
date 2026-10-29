import { toast } from 'svelte-sonner';
import { api, errorMessage } from '#lib/api.js';
import { importViewQuery } from '#lib/view-builder.js';
import { catalog } from '#lib/state/catalog.js';
import { tabs } from '#lib/state/tabs.svelte.js';

export function newView(connectionId: string, database: string, schema: string) {
	const tab = tabs.openQuery(connectionId, database);
	tab.title = 'New view';
	tab.viewSchema = schema;
	tab.creatingView = true;
	tab.builderOpen = true;
}

export async function editView(
	connectionId: string,
	database: string,
	schema: string,
	name: string
) {
	try {
		const [details, tables] = await Promise.all([
			api.viewDetails(connectionId, database, schema, name),
			catalog.completionTables(connectionId, database)
		]);
		const model = importViewQuery(details.query, tables, details.sources, details.searchPath);
		if (!model) {
			const sql = await api.relationDefinition(connectionId, database, schema, name);
			const tab = tabs.openQuery(connectionId, database, sql);
			tab.title = `View: ${name}`;
			tab.viewNotice =
				'This definition uses SQL the visual builder cannot edit yet. Edit the SQL below and run it to update the view.';
			return;
		}
		const tab = tabs.openQuery(connectionId, database);
		tab.title = `View: ${name}`;
		tab.builder = { database, model };
		tab.builderOpen = true;
		tab.editingView = { database, schema, name, details };
	} catch (e) {
		toast.error(errorMessage(e));
	}
}
