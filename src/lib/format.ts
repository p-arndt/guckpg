const counter = new Intl.NumberFormat('en-US');

export function formatCount(n: number): string {
	return counter.format(n);
}

export function formatDuration(ms: number): string {
	if (ms < 1000) return `${ms} ms`;
	if (ms < 60_000) return `${(ms / 1000).toFixed(ms < 10_000 ? 2 : 1)} s`;
	const min = Math.floor(ms / 60_000);
	const s = Math.round((ms % 60_000) / 1000);
	return `${min} min ${s} s`;
}

/** Postgres row estimates: rough on purpose, `~1.2k`. */
export function formatEstimate(n: number | null): string {
	if (n === null || n < 0) return '';
	if (n < 1000) return `~${n}`;
	if (n < 1_000_000) return `~${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
	return `~${(n / 1_000_000).toFixed(n < 10_000_000 ? 1 : 0)}M`;
}
