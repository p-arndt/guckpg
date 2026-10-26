import type { Relation, SchemaObject } from './api.js';

export function groupSchemaObjects(objects: SchemaObject[]) {
	return (
		[
			['function', 'Functions'],
			['procedure', 'Procedures'],
			['trigger', 'Triggers'],
			['sequence', 'Sequences']
		] as const
	)
		.map(([kind, label]) => ({
			kind,
			label,
			items: objects.filter((object) => object.kind === kind)
		}))
		.filter((group) => group.items.length > 0);
}

export function groupRelations(relations: Relation[]) {
	return [
		{
			label: 'Tables',
			items: relations.filter((relation) => !['view', 'materialized'].includes(relation.kind))
		},
		{ label: 'Views', items: relations.filter((relation) => relation.kind === 'view') },
		{
			label: 'Materialized views',
			items: relations.filter((relation) => relation.kind === 'materialized')
		}
	].filter((group) => group.items.length > 0);
}
