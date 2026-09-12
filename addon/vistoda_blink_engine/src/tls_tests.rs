use std::{error::Error, sync::Arc, time::Duration};

use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use rustls::{
    RootCertStore, ServerConfig,
    pki_types::{PrivatePkcs8KeyDer, ServerName},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsAcceptor;

// Synthetic identities only: no vendor session is opened by these tests.
async fn handshake(
    name: &str,
    trusted: bool,
    expired: bool,
    production: bool,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_key = KeyPair::generate()?;
    let ca_cert = ca_params.self_signed(&ca_key)?;
    let issuer = Issuer::new(ca_params, ca_key);
    let mut params = CertificateParams::new(vec!["immi.example.invalid".into()])?;
    if expired {
        params.not_before = rcgen::date_time_ymd(2000, 1, 1);
        params.not_after = rcgen::date_time_ymd(2001, 1, 1);
    }
    let key = KeyPair::generate()?;
    let cert = params.signed_by(&key, &issuer)?;
    let provider = rustls::crypto::ring::default_provider();
    let server = ServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )?;
    let mut roots: RootCertStore = webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect();
    if trusted {
        roots.add(ca_cert.der().clone())?;
    }
    let client = if production {
        super::connector()
    } else {
        super::connector_with_roots(roots)
    };
    let (client_io, server_io) = tokio::io::duplex(16_384);
    let server_task = async {
        match TlsAcceptor::from(Arc::new(server)).accept(server_io).await {
            Ok(mut stream) => {
                let mut auth = [0; 4];
                stream.read_exact(&mut auth).await?;
                assert_eq!(&auth, b"auth");
                stream.write_all(b"ok").await?;
                stream.flush().await?;
                Ok::<bool, std::io::Error>(true)
            }
            Err(_) => Ok(false),
        }
    };
    let client_task = async {
        match client
            .connect(ServerName::try_from(name.to_owned())?, client_io)
            .await
        {
            Ok(mut stream) => {
                stream.write_all(b"auth").await?;
                stream.flush().await?;
                let mut ack = [0; 2];
                stream.read_exact(&mut ack).await?;
                assert_eq!(&ack, b"ok");
                Ok::<bool, Box<dyn Error + Send + Sync>>(true)
            }
            Err(_) => Ok(false),
        }
    };
    let (server_result, client_result) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(server_task, client_task)
    })
    .await?;
    let accepted = client_result?;
    assert_eq!(
        server_result?, accepted,
        "authentication bytes must never precede verified TLS"
    );
    Ok(accepted)
}

#[tokio::test]
#[ignore = "operator-supplied endpoint; TLS only, no authentication or media"]
async fn verified_real_endpoint_handshake() -> Result<(), Box<dyn Error + Send + Sync>> {
    let address = std::env::var("BLINK_TLS_TEST_ENDPOINT")?;
    let socket: std::net::SocketAddr = address.parse()?;
    let tcp = tokio::time::timeout(
        Duration::from_secs(8),
        tokio::net::TcpStream::connect(socket),
    )
    .await??;
    let tls = tokio::time::timeout(
        Duration::from_secs(8),
        super::connector().connect(ServerName::IpAddress(socket.ip().into()), tcp),
    )
    .await??;
    assert!(tls.get_ref().1.peer_certificates().is_some());
    Ok(())
}

#[tokio::test]
async fn trusted_matching_certificate_allows_application_data()
-> Result<(), Box<dyn Error + Send + Sync>> {
    assert!(handshake("immi.example.invalid", true, false, false).await?);
    Ok(())
}

#[tokio::test]
async fn wrong_hostname_is_rejected_before_authentication()
-> Result<(), Box<dyn Error + Send + Sync>> {
    assert!(!handshake("attacker.example.invalid", true, false, false).await?);
    Ok(())
}

#[tokio::test]
async fn expired_certificate_is_rejected_before_authentication()
-> Result<(), Box<dyn Error + Send + Sync>> {
    assert!(!handshake("immi.example.invalid", true, true, false).await?);
    Ok(())
}

#[tokio::test]
async fn unknown_ca_is_rejected_before_authentication() -> Result<(), Box<dyn Error + Send + Sync>>
{
    assert!(!handshake("immi.example.invalid", false, false, false).await?);
    Ok(())
}

#[tokio::test]
async fn production_connector_rejects_untrusted_server() -> Result<(), Box<dyn Error + Send + Sync>>
{
    assert!(!handshake("immi.example.invalid", true, false, true).await?);
    Ok(())
}
