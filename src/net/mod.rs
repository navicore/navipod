use der_parser::oid::Oid;
use rustls::pki_types::ServerName;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use webpki_roots::TLS_SERVER_ROOTS;
use x509_parser::prelude::*;

pub struct CertificateInfo {
    pub host: String,
    pub is_valid: bool,
    pub expires: ASN1Time,
    pub issued_by: String,
}

/// # Errors
///
/// Will return `Err` if function cannot access network or remote host
pub async fn analyze_tls_certificate(
    host: &str,
) -> Result<CertificateInfo, Box<dyn std::error::Error>> {
    let addr = format!("{host}:443");
    let tcp_stream = TcpStream::connect(addr).await?;

    // Create a root cert store with WebPKI roots
    // In webpki-roots 1.0.0, TLS_SERVER_ROOTS is a &[TrustAnchor]
    // We can use the extend method to add all anchors
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.extend(TLS_SERVER_ROOTS.iter().cloned());

    // Create a client config with the root certificates
    let config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));
    let server_name = ServerName::try_from(host)?.to_owned();

    let tls_stream = connector.connect(server_name, tcp_stream).await?;
    let certificates = tls_stream
        .get_ref()
        .1
        .peer_certificates()
        .ok_or("No certificates found")?;

    let certificate = certificates.first().ok_or("No certificates found")?;

    // Get the DER encoded certificate data
    let der_data = certificate.as_ref();

    let (_, cert) = parse_x509_certificate(der_data).map_err(|_| "Failed to parse certificate")?;

    let validity = cert.validity();
    let not_after = validity.not_after;
    let is_valid = validity.is_valid();

    let expires = not_after;

    let oid_cn = Oid::from(&[2, 5, 4, 3])
        .map_err(|e| format!("Failed to create OID for Common Name: {e:?}"))?;
    let oid_o = Oid::from(&[2, 5, 4, 10])
        .map_err(|e| format!("Failed to create OID for Organization: {e:?}"))?;

    let issuer = cert.tbs_certificate.issuer;
    let mut issued_by_parts = Vec::new();

    for rdn in issuer.iter() {
        for attr in rdn.iter() {
            let attr_oid = attr.attr_type();
            let value = attr.attr_value();

            // Convert OID to a readable string, if known
            let attr_type_string = if *attr_oid == oid_cn {
                "CN"
            } else if *attr_oid == oid_o {
                "O"
            }
            // Add additional comparisons for other known OIDs
            else {
                "_"
            };

            if attr_type_string == "_" {
                continue;
            }

            // Inside your loop where you iterate over the attributes
            let attr_value_string = value.as_str().map_or_else(
                |_| {
                    std::str::from_utf8(value.data).map_or_else(
                        |_| format!("{:?}", value.data),
                        std::string::ToString::to_string,
                    )
                },
                std::string::ToString::to_string,
            );

            if !attr_value_string.is_empty() {
                let formatted = format!("{attr_type_string}: {attr_value_string}");
                issued_by_parts.push(formatted);
            }
        }
    }

    let issued_by = issued_by_parts.join(", ");
    let certificate_info = CertificateInfo {
        host: host.to_string(),
        is_valid,  // Assuming the certificate is valid if the handshake was successful.
        expires,   // Placeholder for actual expiry date
        issued_by, // Placeholder for actual issuer
    };

    Ok(certificate_info)
}
