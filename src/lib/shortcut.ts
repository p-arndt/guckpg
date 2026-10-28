export function shortcut(
	key: string,
	shift = false,
	platform = typeof navigator === 'undefined' ? '' : navigator.platform
): string {
	return /Mac/i.test(platform)
		? `${shift ? '⇧' : ''}⌘${key}`
		: `Ctrl+${shift ? 'Shift+' : ''}${key}`;
}
