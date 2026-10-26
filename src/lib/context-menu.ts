/** Places where the webview's own menu (cut, copy, paste) is useful. */
const NATIVE_MENU_TARGETS =
	'input, textarea, [contenteditable=""], [contenteditable="true"], .cm-editor';

interface MenuTarget {
	closest(selector: string): unknown;
}

/**
 * Whether a right-click should open the webview's native menu. Everywhere else it only
 * offers Reload and Inspect, which make no sense in a desktop app, so it is suppressed.
 * App menus (tree, tabs, grid) open regardless: they handle the event on their trigger.
 * In development Shift+right-click keeps the native menu for Inspect Element.
 */
export function allowNativeContextMenu(
	target: MenuTarget | null,
	options: { hasSelection: boolean; shiftKey: boolean; dev: boolean }
): boolean {
	if (options.dev && options.shiftKey) return true;
	if (options.hasSelection) return true;
	return target?.closest(NATIVE_MENU_TARGETS) != null;
}
