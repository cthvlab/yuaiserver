// Броня космопорта YUAI! Шифруем всё, как сундук с ромом, чтоб шпионы не добрались!
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer}; // Секреты и ключи, как древние свитки!
use rustls::ServerConfig; // Кодекс брони для шлюпок и крейсеров!
use rustls_pemfile::{certs, pkcs8_private_keys}; // Читаем свитки с шифрами!
use quinn::{ServerConfig as QuinnServerConfig}; // Кодекс брони для звездолётов QUIC!
use std::fs::File; // Сундук для свитков!
use std::io::BufReader as StdBufReader; // Лупа для чтения свитков!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use tracing::info; // Рация: кричим о победах!
use serde::{Serialize, Deserialize}; // Читаем и пишем звёздные карты!

// Звёздная карта для брони, где хранятся ключи!
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TlsConfig {
    pub cert_path: String, // Путь к свитку с печатями!
    pub key_path: String, // Путь к ключу от сундука!
}

// Комплект брони для шлюпок, крейсеров и звездолётов!
pub struct TlsSettings {
    pub quinn_config: QuinnServerConfig, // Броня для QUIC!
    pub acceptor: tokio_rustls::TlsAcceptor, // Броня для HTTP/HTTPS!
}

impl TlsConfig {
    // Достаём печати и ключ из сундука!
    fn load_certs_and_key<'a>(cert_path: &str, key_path: &str) -> Result<(Vec<CertificateDer<'a>>, PrivateKeyDer<'a>), Box<dyn std::error::Error>> {
        info!("Грузим шифры из сундука, капитан!"); // Доклад в рацию!
        let cert_file = File::open(cert_path)?; // Открываем свиток с печатями!
        let key_file = File::open(key_path)?; // Открываем сундук с ключом!
        
        let certs: Vec<CertificateDer<'a>> = certs(&mut StdBufReader::new(cert_file))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(CertificateDer::from)
            .collect(); // Читаем печати, как звёзды!
        
        let keys: Vec<PrivateKeyDer<'a>> = pkcs8_private_keys(&mut StdBufReader::new(key_file))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|bytes| PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(bytes)))
            .collect(); // Читаем ключи, как древние карты!
        
        let key = keys.into_iter().next().ok_or("Ключ пропал!")?; // Берём первый ключ или кричим о шторме!
        Ok((certs, key)) // Печати и ключ готовы!
    }

    // Куём броню для шлюпок и крейсеров (HTTP/HTTPS)!
    pub fn build_server_config(&self) -> Result<ServerConfig, Box<dyn std::error::Error>> {
        let (certs, key) = Self::load_certs_and_key(&self.cert_path, &self.key_path)?; // Достаём печати и ключ!
        let mut cfg = ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
            .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])? // Поддерживаем TLS 1.3 и 1.2!
            .with_no_client_auth() // Без проверки гостей, все свои!
            .with_single_cert(certs, key)?; // Устанавливаем печати и ключ!
        cfg.alpn_protocols = vec![b"h3".to_vec(), b"h2".to_vec(), b"http/1.1".to_vec()]; // Протоколы: HTTP/3, HTTP/2, HTTP/1.1!
        info!("ALPN настроен: HTTP/3, HTTP/2, HTTP/1.1!"); // Доклад: броня готова!
        Ok(cfg)
    }

    // Куём броню для звездолётов QUIC!
    pub fn build_quinn_config(&self) -> Result<QuinnServerConfig, Box<dyn std::error::Error>> {
        info!("Готовим шифры для QUIC!"); // Доклад в рацию!
        let (certs, key) = Self::load_certs_and_key(&self.cert_path, &self.key_path)?; // Достаём печати и ключ!
        let mut quinn_config = QuinnServerConfig::with_single_cert(certs, key)?; // Устанавливаем броню!
        let mut transport_config = quinn::TransportConfig::default(); // Настройки двигателя!
        transport_config.max_concurrent_bidi_streams(100u32.into()); // До 100 каналов связи!
        quinn_config.transport_config(Arc::new(transport_config)); // Устанавливаем настройки!
        Ok(quinn_config) // Броня для QUIC готова!
    }

    // Собираем полный комплект брони для всех!
    pub fn build_tls_settings(&self) -> Result<TlsSettings, Box<dyn std::error::Error>> {
        let server_config = self.build_server_config()?; // Броня для HTTP/HTTPS!
        let quinn_config = self.build_quinn_config()?; // Броня для QUIC!
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config)); // Преобразуем броню для шлюпок!
        Ok(TlsSettings {
            quinn_config, // Броня QUIC!
            acceptor, // Броня HTTP/HTTPS!
        }) // Комплект готов, шпионы не пройдут!
    }
}
