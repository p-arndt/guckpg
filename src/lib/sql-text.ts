/** A statement inside a script; `from`/`to` are offsets into the script. */
export interface Statement {
	from: number;
	to: number;
	text: string;
}

/**
 * Split a script at top-level semicolons. Semicolons inside strings, quoted
 * identifiers, comments and dollar-quoted bodies (functions, DO blocks) do
 * not split. Statements are trimmed; empty ones are dropped.
 */
export function splitStatements(sql: string): Statement[] {
	const out: Statement[] = [];
	let start = 0;
	let i = 0;
	const n = sql.length;
	let hasCode = false;

	const push = (end: number) => {
		let from = start;
		let to = end;
		while (from < to && /\s/.test(sql[from])) from++;
		while (to > from && /\s/.test(sql[to - 1])) to--;
		if (hasCode) out.push({ from, to, text: sql.slice(from, to) });
		hasCode = false;
	};

	while (i < n) {
		const c = sql[i];
		const next = sql[i + 1];

		if (c === '-' && next === '-') {
			const end = sql.indexOf('\n', i);
			i = end === -1 ? n : end + 1;
		} else if (c === '/' && next === '*') {
			// Postgres block comments nest.
			let depth = 1;
			i += 2;
			while (i < n && depth > 0) {
				if (sql[i] === '/' && sql[i + 1] === '*') {
					depth++;
					i += 2;
				} else if (sql[i] === '*' && sql[i + 1] === '/') {
					depth--;
					i += 2;
				} else i++;
			}
		} else if (c === "'") {
			hasCode = true;
			const escapes = i > 0 && /[eE]/.test(sql[i - 1]) && !/\w/.test(sql[i - 2] ?? '');
			i++;
			while (i < n) {
				if (escapes && sql[i] === '\\') i += 2;
				else if (sql[i] === "'" && sql[i + 1] === "'") i += 2;
				else if (sql[i] === "'") {
					i++;
					break;
				} else i++;
			}
		} else if (c === '"') {
			hasCode = true;
			i++;
			while (i < n) {
				if (sql[i] === '"' && sql[i + 1] === '"') i += 2;
				else if (sql[i] === '"') {
					i++;
					break;
				} else i++;
			}
		} else if (c === '$' && !/\w/.test(sql[i - 1] ?? '')) {
			hasCode = true;
			const tag = /^\$([A-Za-z_][A-Za-z0-9_]*)?\$/.exec(sql.slice(i));
			if (tag) {
				const close = sql.indexOf(tag[0], i + tag[0].length);
				i = close === -1 ? n : close + tag[0].length;
			} else i++;
		} else if (c === ';') {
			push(i);
			start = i + 1;
			i++;
		} else {
			if (!/\s/.test(c)) hasCode = true;
			i++;
		}
	}
	push(n);
	return out;
}

/**
 * The statement the cursor is in. A cursor in the whitespace after a
 * statement's semicolon still means that statement, like in DataGrip.
 */
export function statementAt(sql: string, pos: number): Statement | undefined {
	const all = splitStatements(sql);
	let best: Statement | undefined;
	for (const s of all) {
		if (s.from <= pos) best = s;
		if (pos <= s.to) break;
	}
	return best ?? all[0];
}
