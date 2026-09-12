use std::sync::Arc;

use rustls::{
    ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
    client::{
        WebPkiServerVerifier,
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    },
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use tokio_rustls::TlsConnector;

pub fn connector() -> TlsConnector {
    // Authenticate the IMMI server before sending any session material.
    connector_with_roots(webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect())
}

fn connector_with_roots(roots: RootCertStore) -> TlsConnector {
    let provider = rustls::crypto::ring::default_provider();
    let public =
        WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::new(provider.clone()))
            .build()
            .unwrap_or_else(|_| unreachable!("configured root store is nonempty"));
    let config = ClientConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .unwrap_or_else(|_| unreachable!("ring supports TLS"))
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(ImmiVerifier { public }))
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

#[derive(Debug)]
struct ImmiVerifier {
    public: Arc<WebPkiServerVerifier>,
}

impl ServerCertVerifier for ImmiVerifier {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if crate::tls_pins::trusted(cert, name, now) {
            return Ok(ServerCertVerified::assertion());
        }
        // Unknown/rotated private certificates fail closed. Public PKI still
        // requires the requested DNS/IP identity, chain and certificate validity.
        self.public
            .verify_server_cert(cert, intermediates, name, ocsp, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.public.verify_tls12_signature(message, cert, signature)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.public.verify_tls13_signature(message, cert, signature)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.public.supported_verify_schemes()
    }
}

#[cfg(test)]
#[path = "tls_tests.rs"]
mod tests;
