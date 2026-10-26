use std::collections::HashSet;

use futures_util::{SinkExt, StreamExt, pin_mut};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::session::Manager;
use crate::sql::{qualified, quote_ident};

const MAX_BYTES: usize = 16 * 1024 * 1024;

fn header(csv: &str) -> AppResult<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = csv.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() && !closed => quoted = true,
                ',' => {
                    fields.push(std::mem::take(&mut field));
                    closed = false;
                }
                '\n' | '\r' => break,
                _ if closed || c == '"' => return Err(AppError::msg("Invalid CSV header.")),
                _ => field.push(c),
            }
        }
    }
    if quoted {
        return Err(AppError::msg("Unterminated quoted CSV header."));
    }
    fields.push(field);
    let mut seen = HashSet::new();
    if fields
        .iter()
        .any(|f| f.is_empty() || f.contains('\0') || !seen.insert(f))
    {
        return Err(AppError::msg("CSV headers must be nonempty and unique."));
    }
    Ok(fields)
}

#[tauri::command]
pub async fn import_csv(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    table: String,
    csv: String,
) -> AppResult<u64> {
    if manager.store.get(&id)?.read_only {
        return Err(AppError::msg("This connection is read-only."));
    }
    if csv.len() > MAX_BYTES {
        return Err(AppError::msg("CSV file exceeds the 16 MiB limit."));
    }
    let columns = header(&csv)?
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "COPY {} ({columns}) FROM STDIN WITH (FORMAT csv, HEADER true)",
        qualified(&schema, &table)
    );
    let conn = manager.open(&id, &database).await?;
    let sink = conn.client.copy_in(&sql).await?;
    pin_mut!(sink);
    sink.as_mut()
        .send(std::io::Cursor::new(csv.into_bytes()))
        .await?;
    Ok(sink.finish().await?)
}

#[tauri::command]
pub async fn export_csv(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    table: String,
) -> AppResult<String> {
    let sql = format!(
        "COPY (SELECT * FROM {}) TO STDOUT WITH (FORMAT csv, HEADER true)",
        qualified(&schema, &table)
    );
    let conn = manager.open(&id, &database).await?;
    let stream = conn.client.copy_out(&sql).await?;
    pin_mut!(stream);
    let mut data = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if data.len() + chunk.len() > MAX_BYTES {
            return Err(AppError::msg("CSV export exceeds the 16 MiB limit."));
        }
        data.extend_from_slice(&chunk);
    }
    String::from_utf8(data).map_err(|_| AppError::msg("CSV export is not UTF-8."))
}

#[cfg(test)]
mod tests {
    use super::header;

    #[test]
    fn parses_quoted_header_and_rejects_invalid_names() {
        assert_eq!(
            header("\"a,b\",\"x\"\"y\",plain\r\n1,2,3").unwrap(),
            ["a,b", "x\"y", "plain"]
        );
        assert_eq!(header("\"a\nb\",c\n").unwrap(), ["a\nb", "c"]);
        for input in [
            "",
            "a,a\n",
            "a,,c\n",
            "\"unclosed",
            "\"a\"x,b\n",
            "a\"b,c\n",
        ] {
            assert!(header(input).is_err(), "{input:?}");
        }
    }
}
