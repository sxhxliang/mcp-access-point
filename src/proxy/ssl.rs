use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use dashmap::DashMap;
use log::{debug, error};
use matchit::{InsertError, Router as MatchRouter};
use once_cell::sync::Lazy;
use pingora::listeners::TlsAccept;
use pingora::tls::ext;
use pingora::tls::pkey::PKey;
use pingora::tls::ssl::{NameType, SslRef};
use pingora::tls::x509::X509;
use pingora_error::Result;

use crate::config::{self, Identifiable};

use super::MapOperations;

static DEFAULT_SERVER_NAME: &str = "*";

/// Proxy SSL.
pub struct ProxySSL {
    pub inner: config::SSL,
    // Store parsed cert and key, handle parsing errors during creation/update
    parsed_cert: Result<X509, String>,
    parsed_key: Result<PKey<pingora::tls::pkey::Private>, String>,
}

impl From<config::SSL> for ProxySSL {
    fn from(value: config::SSL) -> Self {
        let parsed_cert = X509::from_pem(value.cert.as_bytes())
            .map_err(|e| format!("Failed to parse cert for {}: {}", value.id, e));
        let parsed_key = PKey::private_key_from_pem(value.key.as_bytes())
            .map_err(|e| format!("Failed to parse key for {}: {}", value.id, e));
        Self {
            inner: value,
            parsed_cert,
            parsed_key,
        }
    }
}

impl Identifiable for ProxySSL {
    fn id(&self) -> &str {
        &self.inner.id
    }

    fn set_id(&mut self, id: String) {
        self.inner.id = id;
    }
}

impl ProxySSL {
    /// Gets the list of SNIs for the SSL.
    fn get_snis(&self) -> Vec<String> {
        self.inner.snis.clone()
    }
}

#[derive(Default)]
pub struct MatchEntry {
    snis: MatchRouter<Arc<ProxySSL>>,
}

impl MatchEntry {
    /// Inserts an SSL into the match entry.
    fn insert_ssl(&mut self, proxy_ssl: Arc<ProxySSL>) -> Result<(), InsertError> {
        let snis = proxy_ssl.get_snis();

        // Insert for host URIs
        for sni in snis.iter() {
            // Reverse SNI to support wildcard matching (e.g., "*.example.com" matches subdomains)
            let reversed_sni = sni.chars().rev().collect::<String>();
            self.snis.insert(reversed_sni, proxy_ssl.clone())?;
        }

        Ok(())
    }

    /// Matches an SNI to an SSL.
    fn match_sni(&self, sni: String) -> Option<Arc<ProxySSL>> {
        // Reverse SNI to match the stored reversed SNI patterns
        let reversed_sni = sni.chars().rev().collect::<String>();

        log::debug!("match sni: sni={sni:?}");

        if let Ok(v) = self.snis.at(&reversed_sni) {
            return Some(v.value.clone());
        }
        None
    }
}

/// Global map to store SSL, initialized lazily.
pub static SSL_MAP: Lazy<DashMap<String, Arc<ProxySSL>>> = Lazy::new(DashMap::new);
static GLOBAL_SSL_MATCH: Lazy<ArcSwap<MatchEntry>> =
    Lazy::new(|| ArcSwap::new(Arc::new(MatchEntry::default())));

fn global_ssl_match_fetch() -> Arc<MatchEntry> {
    GLOBAL_SSL_MATCH.load().clone()
}

pub fn reload_global_ssl_match() {
    let mut matcher = MatchEntry::default();

    for ssl in SSL_MAP.iter() {
        debug!("Inserting route: {}", ssl.value().inner.id);
        matcher.insert_ssl(ssl.value().clone()).unwrap();
    }

    GLOBAL_SSL_MATCH.store(Arc::new(matcher));
}

/// Loads SSL from the given configuration.
pub fn load_static_ssls(config: &config::Config) -> Result<()> {
    let proxy_ssls: Vec<Arc<ProxySSL>> = config
        .ssls
        .iter()
        .filter_map(|ssl| {
            log::info!("Configuring ssl: {}", ssl.id);
            let proxy_ssl = ProxySSL::from(ssl.clone());
            // Only include SSL if both cert and key parsing succeeded
            match (&proxy_ssl.parsed_cert, &proxy_ssl.parsed_key) {
                (Ok(_), Ok(_)) => Some(Ok(Arc::new(proxy_ssl))),
                (Err(e), _) => {
                    error!("{e}");
                    None
                }
                (_, Err(e)) => {
                    error!("{e}");
                    None
                }
            }
        })
        .collect::<Result<Vec<_>>>()?;
    log::info!("Reloading SSL resources");
    SSL_MAP.reload_resources(proxy_ssls);

    reload_global_ssl_match();

    Ok(())
}

pub struct DynamicCert {
    default: Arc<ProxySSL>,
}

impl DynamicCert {
    pub fn new(tls: &config::Tls) -> Box<Self> {
        let cert_bytes =
            std::fs::read(tls.cert_path.clone()).expect("Failed to read TLS certificate file");
        let key_bytes =
            std::fs::read(tls.key_path.clone()).expect("Failed to read TLS private key file");

        let ssl_config = config::SSL {
            id: String::new(),
            cert: String::from_utf8(cert_bytes)
                .expect("Failed to convert certificate bytes to UTF-8 string"),
            key: String::from_utf8(key_bytes)
                .expect("Failed to convert private key bytes to UTF-8 string"),
            snis: Vec::new(),
        };

        let proxy_ssl = ProxySSL::from(ssl_config);
        // Ensure default SSL has valid cert and key
        match (&proxy_ssl.parsed_cert, &proxy_ssl.parsed_key) {
            (Ok(_), Ok(_)) => Box::new(Self {
                default: Arc::new(proxy_ssl),
            }),
            (Err(e), _) => panic!("Default SSL certificate parsing failed: {e}"),
            (_, Err(e)) => panic!("Default SSL key parsing failed: {e}"),
        }
    }
}

#[async_trait]
impl TlsAccept for DynamicCert {
    async fn certificate_callback(&self, ssl: &mut SslRef) {
        let sni = ssl
            .servername(NameType::HOST_NAME)
            .unwrap_or(DEFAULT_SERVER_NAME);

        let proxy_ssl = if let Some(ssl) = global_ssl_match_fetch().match_sni(sni.to_string()) {
            ssl.clone()
        } else {
            self.default.clone()
        };

        match (&proxy_ssl.parsed_cert, &proxy_ssl.parsed_key) {
            (Ok(cert), Ok(key)) => {
                // Use the cached cert and key
                if let Err(e) = ext::ssl_use_certificate(ssl, cert) {
                    log::error!("Failed to use certificate: {e}");
                }
                if let Err(e) = ext::ssl_use_private_key(ssl, key) {
                    log::error!("Failed to use private key: {e}");
                }
            }
            (Err(e), _) => log::error!("{e}"), // Log parsing error stored in ProxySSL
            (_, Err(e)) => log::error!("{e}"), // Log parsing error stored in ProxySSL
        }
    }
}
