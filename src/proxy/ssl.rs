use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use dashmap::DashMap;
use log::debug;
use matchit::{InsertError, Router as MatchRouter};
use once_cell::sync::Lazy;
use pingora::listeners::TlsAccept;
use pingora::tls::ext;
use pingora::tls::pkey::PKey;
use pingora::tls::ssl::{NameType, SslRef};
use pingora::tls::x509::X509;
use pingora_error::Result;

use crate::config;

use super::{Identifiable, MapOperations};

static DEFAULT_SERVER_NAME: &str = "*";

/// Proxy SSL.
pub struct ProxySSL {
    pub inner: config::SSL,
}

impl From<config::SSL> for ProxySSL {
    fn from(value: config::SSL) -> Self {
        Self { inner: value }
    }
}

impl Identifiable for ProxySSL {
    fn id(&self) -> String {
        self.inner.id.clone()
    }

    fn set_id(&mut self, id: String) {
        self.inner.id = id;
    }
}

impl ProxySSL {
    /// Gets the list of snis for the ssl.
    fn get_snis(&self) -> Vec<String> {
        self.inner.snis.clone()
    }

    fn get_cert_bytes(&self) -> Vec<u8> {
        self.inner.cert.clone().into_bytes()
    }

    fn get_key_bytes(&self) -> Vec<u8> {
        self.inner.key.clone().into_bytes()
    }
}

#[derive(Default)]
pub struct MatchEntry {
    snis: MatchRouter<Arc<ProxySSL>>,
}

impl MatchEntry {
    /// Inserts a ssl into the match entry.
    fn insert_ssl(&mut self, proxy_ssl: Arc<ProxySSL>) -> Result<(), InsertError> {
        let snis = proxy_ssl.get_snis();

        // Insert for host URIs
        for sni in snis.iter() {
            let reversed_sni = sni.chars().rev().collect::<String>();
            self.snis.insert(reversed_sni, proxy_ssl.clone())?;
        }

        Ok(())
    }

    /// Matches a sni to a ssl.
    fn match_sni(&self, sni: String) -> Option<Arc<ProxySSL>> {
        let reversed_sni = sni.chars().rev().collect::<String>();

        log::debug!("match sni: sni={:?}", sni,);

        if let Ok(v) = self.snis.at(&reversed_sni) {
            return Some(v.value.clone());
        }
        None
    }
}

/// Global map to store ssl, initialized lazily.
pub static SSL_MAP: Lazy<DashMap<String, Arc<ProxySSL>>> = Lazy::new(DashMap::new);
static GLOBAL_SSL_MATCH: Lazy<ArcSwap<MatchEntry>> =
    Lazy::new(|| ArcSwap::new(Arc::new(MatchEntry::default())));

pub fn global_ssl_match_fetch() -> Arc<MatchEntry> {
    GLOBAL_SSL_MATCH.load().clone()
}

pub fn reload_global_ssl_match() {
    let mut matcher = MatchEntry::default();

    for ssl in SSL_MAP.iter() {
        debug!("Inserting route: {}", ssl.inner.id);
        matcher.insert_ssl(ssl.clone()).unwrap();
    }

    GLOBAL_SSL_MATCH.store(Arc::new(matcher));
}

/// Loads ssl from the given configuration.
pub fn load_static_ssls(config: &config::Config) -> Result<()> {
    let proxy_ssls: Vec<Arc<ProxySSL>> = config
        .ssls
        .iter()
        .map(|ssl| {
            log::info!("Configuring ssl: {}", ssl.id);
            Ok(Arc::new(ProxySSL::from(ssl.clone())))
        })
        .collect::<Result<Vec<_>>>()?;

    SSL_MAP.reload_resource(proxy_ssls);

    reload_global_ssl_match();

    Ok(())
}

/// Fetches an ssl by its ID.
pub fn ssl_fetch(id: &str) -> Option<Arc<ProxySSL>> {
    match SSL_MAP.get(id) {
        Some(rule) => Some(rule.value().clone()),
        None => {
            log::warn!("Route with id '{}' not found", id);
            None
        }
    }
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

        Box::new(Self {
            default: Arc::new(ProxySSL::from(config::SSL {
                id: String::new(),
                cert: String::from_utf8(cert_bytes)
                    .expect("Failed to convert certificate bytes to UTF-8 string"),
                key: String::from_utf8(key_bytes)
                    .expect("Failed to convert private key bytes to UTF-8 string"),
                snis: Vec::new(),
            })),
        })
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

        match X509::from_pem(&proxy_ssl.get_cert_bytes()) {
            Ok(cert) => match PKey::private_key_from_pem(&proxy_ssl.get_key_bytes()) {
                Ok(key) => {
                    if let Err(e) = ext::ssl_use_certificate(ssl, &cert) {
                        log::error!("Failed to use certificate: {}", e);
                    }
                    if let Err(e) = ext::ssl_use_private_key(ssl, &key) {
                        log::error!("Failed to use private key: {}", e);
                    }
                }
                Err(e) => log::error!("Failed to parse private key: {}", e),
            },
            Err(e) => log::error!("Failed to parse certificate: {}", e),
        }
    }
}
