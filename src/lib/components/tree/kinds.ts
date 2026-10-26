import type { Component } from 'svelte';
import TableIcon from '@lucide/svelte/icons/table-2';
import EyeIcon from '@lucide/svelte/icons/eye';
import LayersIcon from '@lucide/svelte/icons/layers';
import GlobeIcon from '@lucide/svelte/icons/globe';
import SquareFunctionIcon from '@lucide/svelte/icons/square-function';
import WorkflowIcon from '@lucide/svelte/icons/workflow';
import ZapIcon from '@lucide/svelte/icons/zap';
import HashIcon from '@lucide/svelte/icons/hash';
import type { RelationKind, SchemaObject } from '#lib/api.js';

export type TreeKind = RelationKind | SchemaObject['kind'];

interface KindStyle {
	icon: Component<{ class?: string }>;
	/** Full class names so Tailwind picks them up. */
	color: string;
}

const table: KindStyle = { icon: TableIcon, color: 'text-kind-table' };

const styles: Record<TreeKind, KindStyle> = {
	table,
	partitioned: table,
	other: table,
	view: { icon: EyeIcon, color: 'text-kind-view' },
	materialized: { icon: LayersIcon, color: 'text-kind-materialized' },
	foreign: { icon: GlobeIcon, color: 'text-kind-foreign' },
	function: { icon: SquareFunctionIcon, color: 'text-kind-function' },
	procedure: { icon: WorkflowIcon, color: 'text-kind-procedure' },
	trigger: { icon: ZapIcon, color: 'text-kind-trigger' },
	sequence: { icon: HashIcon, color: 'text-kind-sequence' }
};

export function kindStyle(kind: TreeKind): KindStyle {
	return styles[kind] ?? table;
}
