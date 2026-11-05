import { fireEvent, render } from '@testing-library/svelte';
import { EditorView } from '@codemirror/view';
import { expect, it } from 'vitest';
import SqlEditor from './SqlEditor.svelte';

it('indents and outdents selected lines with Tab', async () => {
	const { container } = render(SqlEditor, { value: 'SELECT 1;\nSELECT 2;', autofocus: false });
	const content = container.querySelector<HTMLElement>('.cm-content')!;
	const view = EditorView.findFromDOM(content)!;
	view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } });

	await fireEvent.keyDown(content, { key: 'Tab' });
	expect(view.state.doc.toString()).toBe('  SELECT 1;\n  SELECT 2;');
	await fireEvent.keyDown(content, { key: 'Tab', shiftKey: true });
	expect(view.state.doc.toString()).toBe('SELECT 1;\nSELECT 2;');
});
