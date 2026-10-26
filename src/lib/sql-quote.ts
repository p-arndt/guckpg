/** Postgres reserved key words: these never work as bare identifiers. */
const RESERVED = new Set(
	`all analyse analyze and any array as asc asymmetric authorization binary both case cast check
	collate collation column concurrently constraint create cross current_catalog current_date
	current_role current_schema current_time current_timestamp current_user default deferrable desc
	distinct do else end except false fetch for foreign freeze from full grant group having ilike in
	initially inner intersect into is isnull join lateral leading left like limit localtime
	localtimestamp natural not notnull null offset on only or order outer overlaps placing primary
	references returning right select session_user similar some symmetric system_user table
	tablesample then to trailing true union unique user using variadic verbose when where window with`.split(
		/\s+/
	)
);

/** An identifier in double quotes, always. */
export function quoteIdent(name: string): string {
	return `"${name.replaceAll('"', '""')}"`;
}

/** An identifier as written by hand: bare when Postgres would read it back the same. */
export function ident(name: string): string {
	if (/^[a-z_][a-z0-9_$]*$/.test(name) && !RESERVED.has(name)) return name;
	return quoteIdent(name);
}

/** `schema.name`, leaving out `public` because it is on the default search path. */
export function qualified(schema: string, name: string): string {
	return schema === 'public' ? ident(name) : `${ident(schema)}.${ident(name)}`;
}

/**
 * A string literal. Backslashes switch to an E'' literal so the text means the
 * same whatever `standard_conforming_strings` says; matches `quote_literal` in
 * src-tauri/src/sql.rs.
 */
export function literal(value: string): string {
	const quoted = value.replaceAll("'", "''");
	return value.includes('\\') ? `E'${quoted.replaceAll('\\', '\\\\')}'` : `'${quoted}'`;
}

/** Numbers stay bare; everything else is a string literal Postgres casts to the column type. */
export function valueLiteral(value: string): string {
	const trimmed = value.trim();
	if (/^-?\d+(\.\d+)?$/.test(trimmed)) return trimmed;
	return literal(value);
}
