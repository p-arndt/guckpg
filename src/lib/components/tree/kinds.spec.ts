import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { kindStyle, type TreeKind } from './kinds.js';

const distinctKinds: TreeKind[] = [
	'table',
	'view',
	'materialized',
	'foreign',
	'function',
	'procedure',
	'trigger',
	'sequence'
];

describe('tree kind styles', () => {
	it('gives every object kind its own icon and colour', () => {
		const styles = distinctKinds.map(kindStyle);
		expect(new Set(styles.map((style) => style.icon)).size).toBe(distinctKinds.length);
		expect(new Set(styles.map((style) => style.color)).size).toBe(distinctKinds.length);
	});

	it('shows partitioned and unknown relations as tables', () => {
		expect(kindStyle('partitioned')).toBe(kindStyle('table'));
		expect(kindStyle('other')).toBe(kindStyle('table'));
	});

	it('backs every colour class with a light and dark token', () => {
		const tokens = readFileSync('src/routes/tokens.css', 'utf8');
		const theme = readFileSync('src/routes/layout.css', 'utf8');
		for (const kind of distinctKinds) {
			const name = kindStyle(kind).color.replace('text-', '');
			expect(theme).toContain(`--color-${name}:`);
			expect(tokens.match(new RegExp(`--${name}:`, 'g'))).toHaveLength(2);
		}
	});
});
