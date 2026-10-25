use serde::Serialize;

/// Errors crossing the IPC boundary. The frontend switches on `kind`, so
/// host-key prompts and Postgres errors with positions can be rendered
/// differently from plain failures.
#[derive(Debug, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AppError {
    #[error("{message}")]
    Message { message: String },

    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    Postgres {
        message: String,
        code: Option<String>,
        detail: Option<String>,
        hint: Option<String>,
        /// 1-based character position in the submitted SQL.
        position: Option<u32>,
    },

    /// The SSH server presented a key we have never seen. The user decides
    /// whether to trust it; `key` is the OpenSSH line to store if they do.
    #[error("unknown SSH host key for {host}:{port} ({fingerprint})")]
    #[serde(rename_all = "camelCase")]
    UnknownHostKey {
        host: String,
        port: u16,
        fingerprint: String,
        key: String,
    },

    #[error("SSH host key for {host}:{port} has changed ({fingerprint})")]
    #[serde(rename_all = "camelCase")]
    HostKeyChanged {
        host: String,
        port: u16,
        fingerprint: String,
    },
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn msg(message: impl Into<String>) -> Self {
        AppError::Message {
            message: message.into(),
        }
    }
}

impl From<tokio_postgres::Error> for AppError {
    fn from(err: tokio_postgres::Error) -> Self {
        match err.as_db_error() {
            Some(db) => AppError::Postgres {
                message: db.message().to_string(),
                code: Some(db.code().code().to_string()),
                detail: db.detail().map(str::to_string),
                hint: db.hint().map(str::to_string),
                position: match db.position() {
                    Some(tokio_postgres::error::ErrorPosition::Original(p)) => Some(*p),
                    _ => None,
                },
            },
            None => AppError::Postgres {
                message: error_chain(&err),
                code: err.code().map(|c| c.code().to_string()),
                detail: None,
                hint: None,
                position: None,
            },
        }
    }
}

impl From<russh::Error> for AppError {
    fn from(err: russh::Error) -> Self {
        AppError::msg(format!("SSH: {}", error_chain(&err)))
    }
}

impl From<russh::keys::Error> for AppError {
    fn from(err: russh::keys::Error) -> Self {
        AppError::msg(format!("SSH key: {}", error_chain(&err)))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::msg(error_chain(&err))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::msg(format!("invalid data: {err}"))
    }
}

impl From<keyring::Error> for AppError {
    fn from(err: keyring::Error) -> Self {
        AppError::msg(format!("keychain: {err}"))
    }
}

impl From<tauri::Error> for AppError {
    fn from(err: tauri::Error) -> Self {
        AppError::msg(err.to_string())
    }
}

/// tokio-postgres hides the useful part ("connection refused", "password
/// authentication failed") in the source chain, so walk all of it.
fn error_chain(err: &dyn std::error::Error) -> String {
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(s) = source {
        let text = s.to_string();
        if !out.contains(&text) {
            out.push_str(": ");
            out.push_str(&text);
        }
        source = s.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_kind_tag() {
        let json = serde_json::to_value(AppError::msg("boom")).unwrap();
        assert_eq!(json["kind"], "message");
        assert_eq!(json["message"], "boom");
    }

    #[test]
    fn unknown_host_key_uses_camel_case() {
        let json = serde_json::to_value(AppError::UnknownHostKey {
            host: "h".into(),
            port: 22,
            fingerprint: "SHA256:x".into(),
            key: "ssh-ed25519 AAAA".into(),
        })
        .unwrap();
        assert_eq!(json["kind"], "unknownHostKey");
        assert_eq!(json["fingerprint"], "SHA256:x");
    }
}
