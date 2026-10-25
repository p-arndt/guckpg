/**
 * Score how well `query` matches `text` as a case-insensitive subsequence;
 * null when it does not match. Higher is better: consecutive characters and
 * matches at word starts count extra, so "usr" ranks "users" above "audit_user_roles".
 */
export function fuzzyScore(query: string, text: string): number | null {
	const q = query.toLowerCase().replace(/\s+/g, '');
	if (!q) return 0;
	const t = text.toLowerCase();
	let score = 0;
	let ti = 0;
	let streak = 0;
	for (const ch of q) {
		const found = t.indexOf(ch, ti);
		if (found === -1) return null;
		const wordStart = found === 0 || /[\s._\-/:]/.test(t[found - 1]);
		streak = found === ti ? streak + 1 : 0;
		score += 1 + streak * 2 + (wordStart ? 3 : 0);
		ti = found + 1;
	}
	// Prefer shorter texts among equal matches.
	return score - t.length * 0.01;
}

/** Items matching `query`, best first; ties keep their input order. */
export function fuzzyFilter<T>(items: T[], query: string, text: (item: T) => string): T[] {
	return items
		.map((item, index) => ({ item, index, score: fuzzyScore(query, text(item)) }))
		.filter((r): r is { item: T; index: number; score: number } => r.score !== null)
		.sort((a, b) => b.score - a.score || a.index - b.index)
		.map((r) => r.item);
}
