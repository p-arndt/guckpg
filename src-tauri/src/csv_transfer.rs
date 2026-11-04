use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

use futures_util::{SinkExt, StreamExt, pin_mut};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::pg::PgConn;
use crate::session::Manager;
use crate::sql::{qualified, quote_ident};

const CHUNK_SIZE: usize = 64 * 1024;

fn read_header(file: &mut File) -> AppResult<Vec<String>> {
    let mut reader = BufReader::new(&mut *file);
    let mut bytes = Vec::new();
    let mut quoted = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            break;
        }
        let mut consumed = 0;
        let mut finished = false;
        for &byte in buffer {
            consumed += 1;
            bytes.push(byte);
            if byte == b'"' {
                quoted = !quoted;
            } else if !quoted && (byte == b'\n' || byte == b'\r') {
                finished = true;
                break;
            }
        }
        reader.consume(consumed);
        if finished {
            break;
        }
    }
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let text = std::str::from_utf8(if bom { &bytes[3..] } else { &bytes })
        .map_err(|_| AppError::msg("CSV header is not UTF-8."))?;
    let columns = header(text)?;
    file.seek(SeekFrom::Start(if bom { 3 } else { 0 }))?;
    Ok(columns)
}

fn absolute_path(path: &str) -> AppResult<&Path> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(AppError::msg("Enter an absolute file path."));
    }
    Ok(path)
}

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
    path: String,
) -> AppResult<u64> {
    manager.ensure_writable(&id)?;
    let conn = manager.open(&id, &database).await?;
    import_file(&conn, &schema, &table, &path).await
}

async fn import_file(conn: &PgConn, schema: &str, table: &str, path: &str) -> AppResult<u64> {
    let mut file = File::open(absolute_path(path)?)?;
    let columns = read_header(&mut file)?
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "COPY {} ({columns}) FROM STDIN WITH (FORMAT csv, HEADER true)",
        qualified(schema, table)
    );
    let sink = conn.client.copy_in(&sql).await?;
    pin_mut!(sink);
    let mut buffer = vec![0; CHUNK_SIZE];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sink.as_mut()
            .send(std::io::Cursor::new(buffer[..count].to_vec()))
            .await?;
    }
    Ok(sink.finish().await?)
}

#[tauri::command]
pub async fn export_csv(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    table: String,
    path: String,
) -> AppResult<()> {
    let conn = manager.open(&id, &database).await?;
    export_file(&conn, &schema, &table, &path).await
}

async fn export_file(conn: &PgConn, schema: &str, table: &str, path: &str) -> AppResult<()> {
    let sql = format!(
        "COPY (SELECT * FROM {}) TO STDOUT WITH (FORMAT csv, HEADER true)",
        qualified(schema, table)
    );
    let path = absolute_path(path)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let result: AppResult<()> = async {
        let stream = conn.client.copy_out(&sql).await?;
        pin_mut!(stream);
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?)?;
        }
        file.flush()?;
        Ok(())
    }
    .await;
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{absolute_path, header, read_header};
    use std::io::{Seek, Write};

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
    #[tokio::test]
    async fn large_csv_round_trip_against_postgres() {
        let Some((cfg, password)) = crate::testutil::test_db("csv-roundtrip") else {
            return;
        };
        let conn = crate::pg::PgConn::connect(
            &cfg,
            &cfg.database,
            Some(&password),
            crate::pg::Transport::Direct,
        )
        .await
        .unwrap();
        let schema = format!("csv_{}", uuid::Uuid::new_v4().simple());
        let table = crate::sql::qualified(&schema, "records");
        conn.client.batch_execute(&format!("CREATE SCHEMA \"{schema}\"; CREATE TABLE {table} (id integer, value text); INSERT INTO {table} SELECT n, repeat('é', 600) || E'\\n\"quoted\"' FROM generate_series(1, 15000) n")).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.csv");
        let path = path.to_str().unwrap();
        super::export_file(&conn, &schema, "records", path)
            .await
            .unwrap();
        assert!(std::fs::metadata(path).unwrap().len() > 16 * 1024 * 1024);
        assert!(
            super::export_file(&conn, &schema, "records", path)
                .await
                .is_err()
        );
        conn.client
            .batch_execute(&format!("TRUNCATE {table}"))
            .await
            .unwrap();
        assert_eq!(
            super::import_file(&conn, &schema, "records", path)
                .await
                .unwrap(),
            15000
        );
        let row = conn
            .client
            .query_one(
                &format!("SELECT count(*), min(length(value)), max(length(value)) FROM {table}"),
                &[],
            )
            .await
            .unwrap();
        assert_eq!(row.get::<_, i64>(0), 15000);
        assert_eq!(row.get::<_, i32>(1), 609);
        assert_eq!(row.get::<_, i32>(2), 609);
        conn.client
            .batch_execute(&format!("TRUNCATE {table}"))
            .await
            .unwrap();
        let bom_path = dir.path().join("bom.csv");
        std::fs::write(&bom_path, b"\xef\xbb\xbfid,value\n42,hello\n").unwrap();
        assert_eq!(
            super::import_file(&conn, &schema, "records", bom_path.to_str().unwrap())
                .await
                .unwrap(),
            1
        );
        let row = conn
            .client
            .query_one(&format!("SELECT id, value FROM {table}"), &[])
            .await
            .unwrap();
        assert_eq!(row.get::<_, i32>(0), 42);
        assert_eq!(row.get::<_, &str>(1), "hello");
        conn.client
            .batch_execute(&format!("DROP SCHEMA \"{schema}\" CASCADE"))
            .await
            .unwrap();
    }
    #[test]
    fn reads_header_without_loading_large_file() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"\"a\nb\",c\r\n").unwrap();
        file.set_len(17 * 1024 * 1024).unwrap();
        file.rewind().unwrap();
        assert_eq!(read_header(&mut file).unwrap(), ["a\nb", "c"]);
        assert_eq!(file.stream_position().unwrap(), 0);
        assert!(absolute_path("relative.csv").is_err());
    }
}
