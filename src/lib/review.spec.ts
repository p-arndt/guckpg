import { describe, expect, it } from 'vitest';
import { joinStatements, SqlReview } from './review.svelte.js';

describe('joinStatements', () => {
	it('terminates every statement', () => {
		expect(joinStatements(['SELECT 1', 'SELECT 2'])).toBe('SELECT 1;\nSELECT 2;');
		expect(joinStatements(['A', 'B'], '\n\n')).toBe('A;\n\nB;');
		expect(joinStatements([])).toBe('');
	});
});

describe('SqlReview', () => {
	it('opens the dialog with the previewed statements', async () => {
		const review = new SqlReview();
		await expect(review.preview(async () => ['SELECT 1'])).resolves.toBe(true);
		expect(review.sql).toEqual(['SELECT 1']);
		expect(review.reviewing).toBe(true);
		expect(review.busy).toBe(false);
		expect(review.error).toBe('');
	});

	it('reports a failed preview and stays closed', async () => {
		const review = new SqlReview();
		await expect(
			review.preview(async () => {
				throw new Error('boom');
			})
		).resolves.toBe(false);
		expect(review.reviewing).toBe(false);
		expect(review.error).toContain('boom');
	});

	it('runs one request at a time', async () => {
		const review = new SqlReview();
		let release!: () => void;
		const first = review.apply(() => new Promise<void>((resolve) => (release = resolve)));
		expect(review.busy).toBe(true);
		await expect(review.preview(async () => ['x'])).resolves.toBe(false);
		await expect(review.apply(async () => {})).resolves.toBe(false);
		release();
		await expect(first).resolves.toBe(true);
		expect(review.busy).toBe(false);
	});

	it('cannot be dismissed while busy unless forced', async () => {
		const review = new SqlReview();
		await review.preview(async () => ['x']);
		const applied = review.apply(async () => {
			review.close();
			expect(review.reviewing).toBe(true);
			review.close(true);
		});
		await applied;
		expect(review.reviewing).toBe(false);
	});

	it('keeps the dialog open and reports a failed apply', async () => {
		const review = new SqlReview();
		await review.preview(async () => ['x']);
		await expect(
			review.apply(async () => {
				throw new Error('denied');
			})
		).resolves.toBe(false);
		expect(review.reviewing).toBe(true);
		expect(review.error).toContain('denied');
		review.close();
		expect(review.reviewing).toBe(false);
	});
});
