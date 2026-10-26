export interface PlanNode {
	'Node Type': string;
	'Plan Rows'?: number;
	'Total Cost'?: number;
	'Relation Name'?: string;
	'Index Name'?: string;
	Plans?: PlanNode[];
}

export function parsePlan(value: string | null | undefined): PlanNode | null {
	if (!value) return null;
	try {
		const data: unknown = JSON.parse(value);
		if (!Array.isArray(data) || !data[0] || typeof data[0] !== 'object') return null;
		const plan: unknown = data[0].Plan;
		return validNode(plan) ? plan : null;
	} catch {
		return null;
	}
}

function validNode(value: unknown): value is PlanNode {
	if (!value || typeof value !== 'object') return false;
	const node = value as Record<string, unknown>;
	return (
		typeof node['Node Type'] === 'string' &&
		(node.Plans === undefined || (Array.isArray(node.Plans) && node.Plans.every(validNode)))
	);
}
