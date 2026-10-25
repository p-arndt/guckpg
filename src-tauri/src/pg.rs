//! Postgres connections, either direct or through an SSH tunnel, plus query
//! execution. Queries use the simple protocol: values arrive as text exactly
//! as Postgres prints them, every type works without decoding, and a script
//! with several statements returns one result set per statement.

use std::sync::{Arc, LazyLock, OnceLock};
use std::time::{Duration, Instant};

use futures_util::{StreamExt, pin_mut};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use serde::Serialize;
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres::{CancelToken, Client, SimpleQueryMessage};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::config::{ConnectionConfig, SslMode};
use crate::error::{AppError, AppResult};
use crate::ssh::Tunnel;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub enum Transport {
    Direct,
    Tunnel(Arc<Tunnel>),
}

pub struct PgConn {
    pub client: Client,
    cancel: CancelToken,
    transport: Transport,
    host: String,
    port: u16,
    tls: MakeRustlsConnect,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    /// Empty for statements that return no rows (INSERT, CREATE, ...).
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    /// Rows returned or affected, as reported by the server.
    pub row_count: u64,
    /// More rows came back than `max_rows`; only the first ones are kept.
    pub truncated: bool,
    /// Set when the rows can be edited, see `editing::source_info`.
    pub source: Option<crate::editing::SourceInfo>,
}

impl ResultSet {
    fn new(columns: Vec<String>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            source: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryOutput {
    pub results: Vec<ResultSet>,
    pub elapsed_ms: u64,
}

static PROVIDER: LazyLock<Arc<CryptoProvider>> =
    LazyLock::new(|| Arc::new(rustls::crypto::ring::default_provider()));

fn tls_config(mode: SslMode) -> AppResult<ClientConfig> {
    let builder = ClientConfig::builder_with_provider(PROVIDER.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| AppError::msg(format!("TLS: {e}")))?;
    let config = match mode {
        SslMode::VerifyFull => {
            let mut roots = RootCertStore::empty();
            roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
            builder.with_root_certificates(roots).with_no_client_auth()
        }
        // libpq's prefer/require encrypt but do not authenticate the server.
        _ => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertVerification(PROVIDER.clone())))
            .with_no_client_auth(),
    };
    Ok(config)
}

/// Built once per process: every editor tab opens its own connection, and
/// loading the system roots scans the macOS keychain, which takes a while.
static VERIFIED: OnceLock<MakeRustlsConnect> = OnceLock::new();
static UNVERIFIED: OnceLock<MakeRustlsConnect> = OnceLock::new();

fn tls_connector(mode: SslMode) -> AppResult<MakeRustlsConnect> {
    let cell = match mode {
        SslMode::VerifyFull => &VERIFIED,
        SslMode::Disable | SslMode::Prefer | SslMode::Require => &UNVERIFIED,
    };
    if let Some(tls) = cell.get() {
        return Ok(tls.clone());
    }
    let tls = MakeRustlsConnect::new(tls_config(mode)?);
    Ok(cell.get_or_init(|| tls).clone())
}

#[derive(Debug)]
struct NoCertVerification(Arc<CryptoProvider>);

impl ServerCertVerifier for NoCertVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

pub fn pg_config(
    cfg: &ConnectionConfig,
    database: &str,
    password: Option<&str>,
) -> tokio_postgres::Config {
    let mut pg = tokio_postgres::Config::new();
    pg.host(&cfg.host)
        .port(cfg.port)
        .user(&cfg.user)
        .dbname(database)
        .application_name("guckpg")
        .connect_timeout(CONNECT_TIMEOUT)
        .ssl_mode(match cfg.ssl_mode {
            SslMode::Disable => tokio_postgres::config::SslMode::Disable,
            SslMode::Prefer => tokio_postgres::config::SslMode::Prefer,
            SslMode::Require | SslMode::VerifyFull => tokio_postgres::config::SslMode::Require,
        });
    if let Some(pw) = password {
        pg.password(pw);
    }
    if cfg.read_only {
        pg.options("-c default_transaction_read_only=on");
    }
    pg
}

fn spawn_connection<F>(conn: F)
where
    F: std::future::Future<Output = Result<(), tokio_postgres::Error>> + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            log::warn!("postgres connection closed: {e}");
        }
    });
}

impl PgConn {
    pub async fn connect(
        cfg: &ConnectionConfig,
        database: &str,
        password: Option<&str>,
        transport: Transport,
    ) -> AppResult<PgConn> {
        let pg = pg_config(cfg, database, password);
        let tls = tls_connector(cfg.ssl_mode)?;

        let client = match &transport {
            Transport::Direct => {
                let (client, conn) = pg.connect(tls.clone()).await?;
                spawn_connection(conn);
                client
            }
            Transport::Tunnel(tunnel) => {
                let stream = tunnel.open_stream(&cfg.host, cfg.port).await?;
                let connector = tls_for(&tls, &cfg.host)?;
                let (client, conn) =
                    tokio::time::timeout(CONNECT_TIMEOUT, pg.connect_raw(stream, connector))
                        .await
                        .map_err(|_| AppError::msg("timed out talking to Postgres"))??;
                spawn_connection(conn);
                client
            }
        };

        Ok(PgConn {
            cancel: client.cancel_token(),
            client,
            transport,
            host: cfg.host.clone(),
            port: cfg.port,
            tls,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.client.is_closed()
    }

    /// Ask the server to cancel whatever this connection is running. Postgres
    /// needs a second connection for that, through the tunnel if there is one.
    pub async fn cancel(&self) -> AppResult<()> {
        match &self.transport {
            Transport::Direct => self.cancel.cancel_query(self.tls.clone()).await?,
            Transport::Tunnel(tunnel) => {
                let stream = tunnel.open_stream(&self.host, self.port).await?;
                let connector = tls_for(&self.tls, &self.host)?;
                self.cancel.cancel_query_raw(stream, connector).await?
            }
        }
        Ok(())
    }

    /// Run `sql`; a single SELECT-like result also gets its edit source.
    pub async fn run_editable(&self, sql: &str, max_rows: usize) -> AppResult<QueryOutput> {
        let mut output = self.run(sql, max_rows).await?;
        if let [only] = output.results.as_mut_slice()
            && !only.columns.is_empty()
        {
            only.source = crate::editing::source_info(&self.client, sql).await;
        }
        Ok(output)
    }

    pub async fn run(&self, sql: &str, max_rows: usize) -> AppResult<QueryOutput> {
        let started = Instant::now();
        let stream = self.client.simple_query_raw(sql).await?;
        pin_mut!(stream);

        let mut results = Vec::new();
        let mut current: Option<ResultSet> = None;
        while let Some(msg) = stream.next().await {
            match msg? {
                SimpleQueryMessage::RowDescription(cols) => {
                    current = Some(ResultSet::new(
                        cols.iter().map(|c| c.name().to_string()).collect(),
                    ));
                }
                SimpleQueryMessage::Row(row) => {
                    let set = current.get_or_insert_with(|| {
                        ResultSet::new(row.columns().iter().map(|c| c.name().to_string()).collect())
                    });
                    if set.rows.len() < max_rows {
                        set.rows.push(
                            (0..row.len())
                                .map(|i| row.get(i).map(str::to_string))
                                .collect(),
                        );
                    } else {
                        set.truncated = true;
                    }
                }
                SimpleQueryMessage::CommandComplete(count) => {
                    let mut set = current.take().unwrap_or(ResultSet::new(Vec::new()));
                    set.row_count = count;
                    results.push(set);
                }
                _ => {}
            }
        }
        if let Some(set) = current.take() {
            results.push(set);
        }

        Ok(QueryOutput {
            results,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }
}

fn tls_for(
    make: &MakeRustlsConnect,
    host: &str,
) -> AppResult<
    <MakeRustlsConnect as MakeTlsConnect<russh::ChannelStream<russh::client::Msg>>>::TlsConnect,
> {
    let mut make = make.clone();
    MakeTlsConnect::<russh::ChannelStream<russh::client::Msg>>::make_tls_connect(&mut make, host)
        .map_err(|e| AppError::msg(format!("TLS: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ConnectionConfig {
        ConnectionConfig {
            id: "x".into(),
            name: "x".into(),
            host: "db".into(),
            port: 5433,
            user: "app".into(),
            database: "app".into(),
            ssl_mode: SslMode::VerifyFull,
            read_only: true,
            color: None,
            ssh: None,
            folder: None,
        }
    }

    #[test]
    fn config_maps_fields() {
        let pg = pg_config(&sample(), "other", Some("pw"));
        assert_eq!(pg.get_dbname(), Some("other"));
        assert_eq!(pg.get_user(), Some("app"));
        assert_eq!(pg.get_ports(), &[5433]);
        assert_eq!(pg.get_password(), Some(&b"pw"[..]));
        assert_eq!(
            pg.get_options(),
            Some("-c default_transaction_read_only=on")
        );
        assert_eq!(pg.get_ssl_mode(), tokio_postgres::config::SslMode::Require);
    }

    #[test]
    fn tls_configs_build() {
        for mode in [
            SslMode::Disable,
            SslMode::Prefer,
            SslMode::Require,
            SslMode::VerifyFull,
        ] {
            tls_config(mode).unwrap();
        }
    }

    #[test]
    fn tls_connectors_are_cached() {
        tls_connector(SslMode::VerifyFull).unwrap();
        tls_connector(SslMode::Require).unwrap();
        assert!(VERIFIED.get().is_some());
        assert!(UNVERIFIED.get().is_some());
    }
}
