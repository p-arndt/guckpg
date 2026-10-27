//! Import Navicat's XML connection exports and save passwords in the vault.

use std::collections::HashMap;

use aes::Aes128;
use cbc::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
use quick_xml::{Reader, events::Event};
use serde::Serialize;
use tauri::State;

use crate::config::{self, ConnectionConfig, Secrets, SshAuth, SshConfig, SslMode};
use crate::error::{AppError, AppResult};
use crate::session::Manager;

#[derive(Serialize)]
pub struct ImportResult {
    imported: usize,
    skipped: usize,
    warnings: Vec<String>,
}

struct Entry {
    config: ConnectionConfig,
    secrets: Secrets,
    warnings: Vec<String>,
}

type Attributes = HashMap<String, String>;

fn value<'a>(attrs: &'a Attributes, key: &str) -> &'a str {
    attrs.get(key).map_or("", String::as_str)
}

fn enabled(attrs: &Attributes, key: &str) -> bool {
    matches!(
        value(attrs, key).to_ascii_lowercase().as_str(),
        "true" | "1"
    )
}

fn port(attrs: &Attributes, key: &str, fallback: u16) -> Result<u16, &'static str> {
    let input = value(attrs, key);
    if input.is_empty() {
        return Ok(fallback);
    }
    input
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or("invalid port")
}

// NCX v12+ stores each password as hex-encoded AES-128-CBC with PKCS#7 padding.
fn decrypt(input: &str) -> Option<String> {
    if input.is_empty() || !input.len().is_multiple_of(2) || !input.is_ascii() {
        return None;
    }
    let mut bytes = input
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let hex = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(hex, 16).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    let cipher =
        cbc::Decryptor::<Aes128>::new_from_slices(b"libcckeylibcckey", b"libcciv libcciv ").ok()?;
    let plaintext = cipher.decrypt_padded::<Pkcs7>(&mut bytes).ok()?;
    String::from_utf8(plaintext.to_vec()).ok()
}

fn secret(
    attrs: &Attributes,
    key: &str,
    label: &str,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let input = value(attrs, key);
    if input.is_empty() {
        return None;
    }
    match decrypt(input) {
        Some(s) => Some(s),
        None => {
            warnings.push(format!("{label} could not be imported; enter it manually."));
            None
        }
    }
}

fn entry(attrs: &Attributes) -> Result<Entry, &'static str> {
    if enabled(attrs, "HTTP") {
        return Err("HTTP tunnels are not supported");
    }
    if enabled(attrs, "SSL")
        && (!value(attrs, "SSL_ClientKey").is_empty() || !value(attrs, "SSL_ClientCert").is_empty())
    {
        return Err("SSL client certificates are not supported");
    }
    let mut warnings = Vec::new();
    let ssl_mode = if enabled(attrs, "SSL") {
        match value(attrs, "SSL_PGSSLMode").to_ascii_lowercase().as_str() {
            "disable" => SslMode::Disable,
            "prefer" => SslMode::Prefer,
            "require" | "" => SslMode::Require,
            "verify-ca" => SslMode::VerifyCa,
            "verify-full" => SslMode::VerifyFull,
            _ => return Err("unsupported SSL mode"),
        }
    } else {
        SslMode::Disable
    };
    let ssh = if enabled(attrs, "SSH") {
        let auth = match value(attrs, "SSH_AuthenMethod")
            .to_ascii_uppercase()
            .as_str()
        {
            "PASSWORD" | "" => SshAuth::Password,
            "PUBLICKEY" | "PUBLIC KEY" | "KEY" => SshAuth::Key {
                key_path: value(attrs, "SSH_PrivateKey").to_string(),
            },
            "AGENT" => SshAuth::Agent,
            _ => return Err("unsupported SSH authentication"),
        };
        Some(SshConfig {
            host: value(attrs, "SSH_Host").to_string(),
            port: port(attrs, "SSH_Port", 22)?,
            user: value(attrs, "SSH_UserName").to_string(),
            auth,
        })
    } else {
        None
    };
    let db = value(attrs, "Database");
    let config = ConnectionConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: value(attrs, "ConnectionName").to_string(),
        host: value(attrs, "Host").to_string(),
        port: port(attrs, "Port", 5432)?,
        user: value(attrs, "UserName").to_string(),
        database: if db.is_empty() { "postgres" } else { db }.to_string(),
        ssl_mode,
        ssl_root_cert: (!value(attrs, "SSL_CACert").is_empty())
            .then(|| value(attrs, "SSL_CACert").to_string()),
        read_only: false,
        color: None,
        ssh,
        folder: None,
    };
    config
        .validate()
        .map_err(|_| "missing required connection settings")?;
    let secrets = Secrets {
        password: secret(attrs, "Password", "Database password", &mut warnings),
        ssh_password: if config
            .ssh
            .as_ref()
            .is_some_and(|s| matches!(s.auth, SshAuth::Password))
        {
            secret(attrs, "SSH_Password", "SSH password", &mut warnings)
        } else {
            None
        },
        ssh_passphrase: if config
            .ssh
            .as_ref()
            .is_some_and(|s| matches!(s.auth, SshAuth::Key { .. }))
        {
            secret(attrs, "SSH_Passphrase", "SSH key passphrase", &mut warnings)
        } else {
            None
        },
    };
    Ok(Entry {
        config,
        secrets,
        warnings,
    })
}

fn parse(xml: &str) -> AppResult<Vec<Attributes>> {
    if xml.len() > 16 * 1024 * 1024 {
        return Err(AppError::msg("Navicat export exceeds 16 MiB"));
    }
    let mut reader = Reader::from_str(xml);
    let mut root = false;
    let mut depth = 0usize;
    let mut entries = Vec::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|_| AppError::msg("Invalid Navicat XML"))?;
        match event {
            Event::Decl(ref e) => {
                if let Some(encoding) = e.encoding() {
                    let encoding =
                        encoding.map_err(|_| AppError::msg("Invalid XML encoding declaration"))?;
                    if !encoding.eq_ignore_ascii_case("utf-8") {
                        return Err(AppError::msg("Only UTF-8 Navicat exports are supported"));
                    }
                }
            }
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                if depth == 0 {
                    if root || e.name().as_ref() != "Connections" {
                        return Err(AppError::msg("Not a Navicat connections export"));
                    }
                    root = true;
                } else if depth == 1 && e.name().as_ref() == "Connection" {
                    let mut attrs = Attributes::new();
                    for attr in e.attributes() {
                        let attr =
                            attr.map_err(|_| AppError::msg("Invalid Navicat XML attribute"))?;
                        let key = attr.key.as_ref().to_string();
                        let val = attr
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|_| AppError::msg("Invalid Navicat XML attribute"))?
                            .into_owned();
                        attrs.insert(key, val);
                    }
                    entries.push(attrs);
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| AppError::msg("Invalid Navicat XML"))?;
            }
            Event::DocType(_) => return Err(AppError::msg("XML document types are not supported")),
            Event::Eof => break,
            _ => {}
        }
    }
    if !root || depth != 0 {
        return Err(AppError::msg("Invalid Navicat XML"));
    }
    Ok(entries)
}

#[tauri::command]
pub fn import_navicat(manager: State<'_, Manager>, xml: String) -> AppResult<ImportResult> {
    let entries = parse(&xml)?;
    let mut existing = manager
        .store
        .list()?
        .into_iter()
        .map(|c| c.name)
        .collect::<Vec<_>>();
    let mut result = ImportResult {
        imported: 0,
        skipped: 0,
        warnings: Vec::new(),
    };
    for attrs in entries {
        if !value(&attrs, "ConnType").eq_ignore_ascii_case("POSTGRESQL") {
            result.skipped += 1;
            continue;
        }
        let name = value(&attrs, "ConnectionName");
        if existing.iter().any(|n| n == name) {
            result.skipped += 1;
            result.warnings.push(format!(
                "{name}: a connection with this name already exists."
            ));
            continue;
        }
        let entry = match entry(&attrs) {
            Ok(entry) => entry,
            Err(reason) => {
                result.skipped += 1;
                result.warnings.push(format!("{name}: {reason}."));
                continue;
            }
        };
        if let Err(err) = config::store_secrets_for(&entry.config, None, &entry.secrets) {
            result.skipped += 1;
            result
                .warnings
                .push(format!("{name}: could not save credentials ({err})."));
            continue;
        }
        if let Err(err) = manager.store.upsert(entry.config.clone()) {
            let _ = config::delete_secrets(&entry.config.id);
            result.skipped += 1;
            result
                .warnings
                .push(format!("{name}: could not save connection ({err})."));
            continue;
        }
        result
            .warnings
            .extend(entry.warnings.into_iter().map(|w| format!("{name}: {w}")));
        existing.push(entry.config.name);
        result.imported += 1;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_postgres_ssh_and_escaped_names() {
        let xml = r#"<?xml version="1.0"?><Connections Ver="1.4"><Connection ConnType="POSTGRESQL" ConnectionName="Sales &amp; Co" Host="db" Port="5433" Database="shop" UserName="alice" SSL="true" SSL_PGSSLMode="VERIFY-FULL" SSL_CACert="/ca.pem" SSH="true" SSH_Host="jump" SSH_Port="2222" SSH_UserName="bob" SSH_AuthenMethod="PUBLICKEY" SSH_PrivateKey="/key"/></Connections>"#;
        let entries = parse(xml).unwrap();
        let item = entry(&entries[0]).unwrap();
        assert_eq!(item.config.name, "Sales & Co");
        assert_eq!(item.config.database, "shop");
        assert_eq!(item.config.ssl_mode, SslMode::VerifyFull);
        let ssh = item.config.ssh.unwrap();
        assert_eq!(ssh.port, 2222);
        assert!(matches!(ssh.auth, SshAuth::Key { key_path } if key_path == "/key"));
    }

    #[test]
    fn decrypts_modern_ncx_secrets_and_warns_on_invalid_data() {
        use cbc::cipher::BlockModeEncrypt;
        let mut buf = [0u8; 32];
        buf[..6].copy_from_slice(b"secret");
        let encrypted =
            cbc::Encryptor::<Aes128>::new_from_slices(b"libcckeylibcckey", b"libcciv libcciv ")
                .unwrap()
                .encrypt_padded::<Pkcs7>(&mut buf, 6)
                .unwrap();
        let hex = encrypted
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>();
        assert_eq!(decrypt(&hex).as_deref(), Some("secret"));
        let mut warnings = Vec::new();
        let attrs = Attributes::from([(String::from("Password"), String::from("bad"))]);
        assert_eq!(secret(&attrs, "Password", "Password", &mut warnings), None);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn rejects_bad_xml_and_unsupported_transport() {
        assert!(parse("<Other/>").is_err());
        assert!(parse("<Connections><Connection").is_err());
        assert!(parse("<!DOCTYPE x><Connections/>").is_err());
        assert!(parse("<?xml version='1.0' encoding='iso-8859-1'?><Connections/>").is_err());
        let attrs = Attributes::from([(String::from("HTTP"), String::from("true"))]);
        assert!(entry(&attrs).is_err());
    }

    #[test]
    fn reads_navicat_14_postgres_export_attributes() {
        // Format from https://gist.github.com/ericdagenais/69265926b3304314dbbc9f60a76032ba
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><Connections Ver="1.4"><Connection ConnectionName="example" ConnType="POSTGRESQL" Host="localhost" Port="5432" Database="postgres" UserName="postgres" Password="" SavePassword="true" SSL="false" SSL_PGSSLMode="REQUIRE" SSH="false" SSH_AuthenMethod="PASSWORD" SSH_Port="22" SSH_Password="" SSH_PrivateKey="" SSH_Passphrase="" HTTP="false"/></Connections>"#;
        let entries = parse(xml).unwrap();
        let item = entry(&entries[0]).unwrap();
        assert_eq!(item.config.name, "example");
        assert_eq!(item.config.ssl_mode, SslMode::Disable);
        assert!(item.config.ssh.is_none());
    }
}
