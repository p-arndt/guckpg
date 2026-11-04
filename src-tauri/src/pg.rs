//! Postgres connections, either direct or through an SSH tunnel, plus query
//! execution. Queries use the simple protocol: values arrive as text exactly
//! as Postgres prints them, every type works without decoding, and a script
//! with several statements returns one result set per statement.

use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::{Duration, Instant};

use futures_util::{StreamExt, pin_mut};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
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
    result_cache: tokio::sync::Mutex<Option<ResultCache>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_id: Option<String>,
}

struct CachedSet {
    file: Option<BufWriter<std::fs::File>>,
    offsets: Vec<u64>,
    count: usize,
}

impl CachedSet {
    fn new() -> AppResult<Self> {
        Ok(Self {
            file: None,
            offsets: Vec::new(),
            count: 0,
        })
    }

    fn push(&mut self, row: &[Option<String>], page_size: usize) -> AppResult<()> {
        if self.file.is_none() {
            self.file = Some(BufWriter::new(tempfile::tempfile()?));
        }
        let file = self.file.as_mut().expect("file initialized");
        if self.count.is_multiple_of(page_size) {
            self.offsets.push(file.stream_position()?);
        }
        serde_json::to_writer(&mut *file, row)?;
        file.write_all(b"\n")?;
        self.count += 1;
        Ok(())
    }

    fn page(&mut self, page: usize, page_size: usize) -> AppResult<Vec<Vec<Option<String>>>> {
        let offset = *self
            .offsets
            .get(page)
            .ok_or_else(|| AppError::msg("result page does not exist"))?;
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| AppError::msg("result page does not exist"))?;
        file.seek(SeekFrom::Start(offset))?;
        let mut reader = BufReader::new(file.get_mut());
        let mut rows = Vec::new();
        for _ in 0..page_size {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            rows.push(serde_json::from_str(&line)?);
        }
        Ok(rows)
    }
}

struct ResultCache {
    id: String,
    page_size: usize,
    sets: Vec<CachedSet>,
}

static PROVIDER: LazyLock<Arc<CryptoProvider>> =
    LazyLock::new(|| Arc::new(rustls::crypto::ring::default_provider()));

/// The CA file from the config, if one is set.
fn root_cert_path(cfg_value: Option<&str>) -> Option<&str> {
    cfg_value.map(str::trim).filter(|p| !p.is_empty())
}

/// Trust anchors for the verifying modes: the configured CA file replaces
/// the system roots, like libpq's `sslrootcert`.
fn roots(root_cert: Option<&str>) -> AppResult<RootCertStore> {
    let mut roots = RootCertStore::empty();
    match root_cert {
        Some(path) => {
            use rustls::pki_types::pem::PemObject;
            let path = crate::ssh::expand_home(path);
            let pem = std::fs::read(&path).map_err(|e| {
                AppError::msg(format!("TLS root certificate {}: {e}", path.display()))
            })?;
            let certs = CertificateDer::pem_slice_iter(&pem)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    AppError::msg(format!("TLS root certificate {}: {e}", path.display()))
                })?;
            let (added, _) = roots.add_parsable_certificates(certs);
            if added == 0 {
                return Err(AppError::msg(format!(
                    "TLS root certificate {}: no usable certificate found",
                    path.display()
                )));
            }
        }
        None => {
            roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        }
    }
    Ok(roots)
}

fn verifier(mode: SslMode, root_cert: Option<&str>) -> AppResult<Arc<dyn ServerCertVerifier>> {
    let webpki = || {
        WebPkiServerVerifier::builder_with_provider(Arc::new(roots(root_cert)?), PROVIDER.clone())
            .build()
            .map_err(|e| AppError::msg(format!("TLS: {e}")))
    };
    Ok(match mode {
        SslMode::VerifyFull => webpki()?,
        SslMode::VerifyCa => Arc::new(ChainOnly(webpki()?)),
        // libpq's prefer/require encrypt but do not authenticate the server.
        SslMode::Disable | SslMode::Prefer | SslMode::Require => {
            Arc::new(NoCertVerification(PROVIDER.clone()))
        }
    })
}

fn tls_config(mode: SslMode, root_cert: Option<&str>) -> AppResult<ClientConfig> {
    Ok(ClientConfig::builder_with_provider(PROVIDER.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| AppError::msg(format!("TLS: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(verifier(mode, root_cert)?)
        .with_no_client_auth())
}

/// Built once per process: every editor tab opens its own connection, and
/// loading the system roots scans the macOS keychain, which takes a while.
/// Configs with their own CA file are built per connection instead, so an
/// updated file is picked up without a restart.
static VERIFIED: OnceLock<MakeRustlsConnect> = OnceLock::new();
static VERIFIED_CA: OnceLock<MakeRustlsConnect> = OnceLock::new();
static UNVERIFIED: OnceLock<MakeRustlsConnect> = OnceLock::new();

fn tls_connector_blocking(mode: SslMode, root_cert: Option<&str>) -> AppResult<MakeRustlsConnect> {
    if root_cert.is_some() && mode.verifies() {
        return Ok(MakeRustlsConnect::new(tls_config(mode, root_cert)?));
    }
    let cell = match mode {
        SslMode::VerifyFull => &VERIFIED,
        SslMode::VerifyCa => &VERIFIED_CA,
        SslMode::Disable | SslMode::Prefer | SslMode::Require => &UNVERIFIED,
    };
    if let Some(tls) = cell.get() {
        return Ok(tls.clone());
    }
    let tls = MakeRustlsConnect::new(tls_config(mode, None)?);
    Ok(cell.get_or_init(|| tls).clone())
}

/// Reading certificate files and the system trust store is blocking IO.
async fn tls_connector(mode: SslMode, root_cert: Option<&str>) -> AppResult<MakeRustlsConnect> {
    let root_cert = root_cert.map(str::to_string);
    tokio::task::spawn_blocking(move || tls_connector_blocking(mode, root_cert.as_deref()))
        .await
        .map_err(|e| AppError::msg(format!("TLS: {e}")))?
}

/// `verify-ca`: the chain must lead to a trusted root, but the certificate
/// may name a different host (servers reached by IP or through a tunnel).
#[derive(Debug)]
struct ChainOnly(Arc<WebPkiServerVerifier>);

impl ServerCertVerifier for ChainOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // WebPKI checks the chain before the name, so a name error means the
        // chain itself was accepted.
        match self
            .0
            .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
        {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
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
        .application_name("pieg")
        .connect_timeout(CONNECT_TIMEOUT)
        .ssl_mode(match cfg.ssl_mode {
            SslMode::Disable => tokio_postgres::config::SslMode::Disable,
            SslMode::Prefer => tokio_postgres::config::SslMode::Prefer,
            SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => {
                tokio_postgres::config::SslMode::Require
            }
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
        let tls = tls_connector(cfg.ssl_mode, root_cert_path(cfg.ssl_root_cert.as_deref())).await?;

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
            result_cache: tokio::sync::Mutex::new(None),
        })
    }

    pub fn is_closed(&self) -> bool {
        self.client.is_closed()
            || matches!(&self.transport, Transport::Tunnel(tunnel) if tunnel.is_closed())
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

    pub async fn run_cached(&self, sql: &str, page_size: usize) -> AppResult<QueryOutput> {
        *self.result_cache.lock().await = None;
        let page_size = page_size.max(1);
        let started = Instant::now();
        let stream = self.client.simple_query_raw(sql).await?;
        pin_mut!(stream);
        let mut results = Vec::new();
        let mut sets = Vec::new();
        let mut current: Option<(ResultSet, CachedSet)> = None;
        while let Some(msg) = stream.next().await {
            match msg? {
                SimpleQueryMessage::RowDescription(cols) => {
                    current = Some((
                        ResultSet::new(cols.iter().map(|c| c.name().to_string()).collect()),
                        CachedSet::new()?,
                    ));
                }
                SimpleQueryMessage::Row(row) => {
                    if current.is_none() {
                        current = Some((
                            ResultSet::new(
                                row.columns().iter().map(|c| c.name().to_string()).collect(),
                            ),
                            CachedSet::new()?,
                        ));
                    }
                    let (set, cache) = current.as_mut().unwrap();
                    let values: Vec<Option<String>> = (0..row.len())
                        .map(|i| row.get(i).map(str::to_string))
                        .collect();
                    if set.rows.len() < page_size {
                        set.rows.push(values);
                    } else {
                        if cache.count == 0 {
                            for row in &set.rows {
                                cache.push(row, page_size)?;
                            }
                        }
                        cache.push(&values, page_size)?;
                    }
                }
                SimpleQueryMessage::CommandComplete(count) => {
                    let (mut set, cache) = match current.take() {
                        Some(value) => value,
                        None => (ResultSet::new(Vec::new()), CachedSet::new()?),
                    };
                    set.row_count = count;
                    set.truncated = cache.count > page_size;
                    results.push(set);
                    sets.push(cache);
                }
                _ => {}
            }
        }
        if let Some((mut set, cache)) = current {
            set.row_count = if cache.count == 0 {
                set.rows.len()
            } else {
                cache.count
            } as u64;
            set.truncated = cache.count > page_size;
            results.push(set);
            sets.push(cache);
        }
        if let [only] = results.as_mut_slice()
            && !only.columns.is_empty()
        {
            only.source = crate::editing::source_info(&self.client, sql).await;
        }
        let id = uuid::Uuid::new_v4().to_string();
        *self.result_cache.lock().await = Some(ResultCache {
            id: id.clone(),
            page_size,
            sets,
        });
        Ok(QueryOutput {
            results,
            elapsed_ms: started.elapsed().as_millis() as u64,
            cache_id: Some(id),
        })
    }

    pub async fn result_page(
        &self,
        cache_id: &str,
        result: usize,
        page: usize,
    ) -> AppResult<Vec<Vec<Option<String>>>> {
        let mut guard = self.result_cache.lock().await;
        let cache = guard
            .as_mut()
            .filter(|cache| cache.id == cache_id)
            .ok_or_else(|| AppError::msg("query results are no longer available"))?;
        let set = cache
            .sets
            .get_mut(result)
            .ok_or_else(|| AppError::msg("result does not exist"))?;
        set.page(page, cache.page_size)
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
            cache_id: None,
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

    #[test]
    fn cached_pages_preserve_values_and_boundaries() {
        let mut cache = CachedSet::new().unwrap();
        assert!(cache.file.is_none());
        let rows = vec![
            vec![Some("first\nline".into()), None],
            vec![Some("second".into())],
            vec![Some("third".into())],
        ];
        for row in &rows {
            cache.push(row, 2).unwrap();
        }
        assert_eq!(cache.page(0, 2).unwrap(), rows[..2]);
        assert_eq!(cache.page(1, 2).unwrap(), rows[2..]);
        assert!(cache.page(2, 2).is_err());
        assert_eq!(cache.page(0, 2).unwrap(), rows[..2]);
    }

    fn sample() -> ConnectionConfig {
        ConnectionConfig {
            host: "db".into(),
            port: 5433,
            user: "app".into(),
            database: "app".into(),
            ssl_mode: SslMode::VerifyFull,
            read_only: true,
            ..crate::testutil::connection("x")
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
            SslMode::VerifyCa,
            SslMode::VerifyFull,
        ] {
            tls_config(mode, None).unwrap();
        }
    }

    #[tokio::test]
    async fn tls_connectors_are_cached() {
        tls_connector(SslMode::VerifyFull, None).await.unwrap();
        tls_connector(SslMode::VerifyCa, None).await.unwrap();
        tls_connector(SslMode::Require, None).await.unwrap();
        assert!(VERIFIED.get().is_some());
        assert!(VERIFIED_CA.get().is_some());
        assert!(UNVERIFIED.get().is_some());
    }

    /// A test CA and a leaf it signed for `db.internal`, valid until 2126.
    const TEST_CA: &str = "-----BEGIN CERTIFICATE-----
MIIBeDCCAR6gAwIBAgIUOAT9kZYyE/Z2Mhpba/GAWgt3xrwwCgYIKoZIzj0EAwIw
GTEXMBUGA1UEAwwOZ3Vja3BnIHRlc3QgQ0EwIBcNMjYxMDI2MjIyNTQ2WhgPMjEy
NjEwMDIyMjI1NDZaMBkxFzAVBgNVBAMMDmd1Y2twZyB0ZXN0IENBMFkwEwYHKoZI
zj0CAQYIKoZIzj0DAQcDQgAEMyfFIMK31cQVJ+9ZPY0YiGQROmN4D8vid5m+yHt2
yW/HEA1cj3dbNcSGiGGZDlxhKC5lXzd2qaiew9F4qPpOJqNCMEAwHQYDVR0OBBYE
FKeqWyJbMrmyBJh5htXXDtg6lHrkMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/
BAQDAgIEMAoGCCqGSM49BAMCA0gAMEUCIBumrByz1pVmr4p8ZnwiWUSRTowVvGpe
BSmmO/V4qcSEAiEAqCPf3NJAtbnbsSNWMM+40OWDC2oqtXv3qGCPDBKIurE=
-----END CERTIFICATE-----
";
    const TEST_LEAF: &str = "-----BEGIN CERTIFICATE-----
MIIBbTCCAROgAwIBAgIUVHAZBPOMrOFxdfoz2D60ollo33owCgYIKoZIzj0EAwIw
GTEXMBUGA1UEAwwOZ3Vja3BnIHRlc3QgQ0EwIBcNMjYxMDI2MjIyNTQ2WhgPMjEy
NjEwMDIyMjI1NDZaMBYxFDASBgNVBAMMC2RiLmludGVybmFsMFkwEwYHKoZIzj0C
AQYIKoZIzj0DAQcDQgAE9V0kuhSMyXNApZsoNxMtBVSiNHyx0tgfBlhj6CTIa06X
pRLECaUJEx387XxfwGMqTifKTPuXlITtCc0KsXdYkqM6MDgwFgYDVR0RBA8wDYIL
ZGIuaW50ZXJuYWwwCQYDVR0TBAIwADATBgNVHSUEDDAKBggrBgEFBQcDATAKBggq
hkjOPQQDAgNIADBFAiAgeyo68rsxIoOwB+wykK6SWHWxh0X7ux210hWwYbFskQIh
AKfgPVBvSO9vRozsAGopsLKhbL8xLNmC/PZFI2sz3uhq
-----END CERTIFICATE-----
";
    const OTHER_CA: &str = "-----BEGIN CERTIFICATE-----
MIIBbDCCARKgAwIBAgIUE5mFqszKZQyivLznRbAb3tekjZcwCgYIKoZIzj0EAwIw
EzERMA8GA1UEAwwIb3RoZXIgQ0EwIBcNMjYxMDI2MjIyNTUwWhgPMjEyNjEwMDIy
MjI1NTBaMBMxETAPBgNVBAMMCG90aGVyIENBMFkwEwYHKoZIzj0CAQYIKoZIzj0D
AQcDQgAEEd3sae90R9Trk4VJ1CbIsTT1L0wn5ULyNR2Cm2PHsdVXvAKrRrMZhrUJ
xJR74FGZWRWCws67hvWqMNwQNcHY26NCMEAwHQYDVR0OBBYEFOm1vebdpmQn0kB5
xMUQmzcQbdrjMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgIEMAoGCCqG
SM49BAMCA0gAMEUCIQD43tETue0DyAnhmncg6AXyen+hR6fJHoa8YaBXdBS9FgIg
b/n5zOOwfBgVoklWiBMYBLZxl6IuHKRbUgBe4mxITe0=
-----END CERTIFICATE-----
";

    fn ca_file(dir: &tempfile::TempDir, name: &str, pem: &str) -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, pem).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn verify(
        mode: SslMode,
        root_cert: &str,
        host: &str,
    ) -> Result<ServerCertVerified, rustls::Error> {
        use rustls::pki_types::pem::PemObject;
        let verifier = verifier(mode, Some(root_cert)).unwrap();
        let leaf = CertificateDer::from_pem_slice(TEST_LEAF.as_bytes()).unwrap();
        // Inside the certificates' validity, independent of the test clock.
        let now = UnixTime::since_unix_epoch(Duration::from_secs(1_893_456_000));
        verifier.verify_server_cert(
            &leaf,
            &[],
            &ServerName::try_from(host.to_string()).unwrap(),
            &[],
            now,
        )
    }

    #[test]
    fn verify_ca_checks_the_chain_but_not_the_name() {
        let dir = tempfile::tempdir().unwrap();
        let ca = ca_file(&dir, "ca.pem", TEST_CA);
        let other = ca_file(&dir, "other.pem", OTHER_CA);

        assert!(verify(SslMode::VerifyCa, &ca, "db.internal").is_ok());
        assert!(verify(SslMode::VerifyCa, &ca, "10.0.0.5").is_ok());
        assert!(verify(SslMode::VerifyCa, &other, "db.internal").is_err());

        assert!(verify(SslMode::VerifyFull, &ca, "db.internal").is_ok());
        assert!(verify(SslMode::VerifyFull, &ca, "10.0.0.5").is_err());
        assert!(verify(SslMode::VerifyFull, &other, "db.internal").is_err());
    }

    #[test]
    fn root_cert_file_errors_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.pem");
        let err = tls_config(SslMode::VerifyFull, missing.to_str()).unwrap_err();
        assert!(err.to_string().contains("missing.pem"), "{err}");

        let garbage = ca_file(&dir, "garbage.pem", "not a certificate");
        assert!(tls_config(SslMode::VerifyCa, Some(&garbage)).is_err());

        assert_eq!(root_cert_path(Some("  ")), None);
        assert_eq!(root_cert_path(Some(" /a.pem ")), Some("/a.pem"));
        assert_eq!(root_cert_path(None), None);
    }

    #[test]
    fn verify_ca_requires_tls() {
        let mut cfg = sample();
        cfg.ssl_mode = SslMode::VerifyCa;
        assert_eq!(
            pg_config(&cfg, "app", None).get_ssl_mode(),
            tokio_postgres::config::SslMode::Require
        );
    }
}
