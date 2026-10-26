import { describe, expect, it } from 'vitest';
import { parsePlan } from './explain.js';

describe('parsePlan', () => {
	it('reads nested PostgreSQL plans', () => {
		const plan = { 'Node Type': 'Aggregate', Plans: [{ 'Node Type': 'Seq Scan', 'Plan Rows': 4 }] };
		expect(parsePlan(JSON.stringify([{ Plan: plan }]))).toEqual(plan);
	});
	it('rejects malformed plans', () => {
		expect(parsePlan('invalid')).toBeNull();
		expect(parsePlan('[{"Plan":{"Node Type":"Scan","Plans":[{}]}}]')).toBeNull();
	});
});
