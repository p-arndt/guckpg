export function zoomForKey(
	event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey'>,
	zoom: number
): number | null {
	if ((!event.ctrlKey && !event.metaKey) || event.altKey) return null;
	if (event.key === '0') return 1;
	if (event.key === '+' || event.key === '=')
		return Math.min(2, Math.round((zoom + 0.1) * 10) / 10);
	if (event.key === '-') return Math.max(0.5, Math.round((zoom - 0.1) * 10) / 10);
	return null;
}
