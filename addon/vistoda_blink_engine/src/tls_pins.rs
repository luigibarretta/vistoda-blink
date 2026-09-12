//! Exact certificate identities from authenticated Blink Android trust material.
//! No certificates, vendor code, private keys or trust-on-first-use are embedded.
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};

// Intersection of all pinned certificates' validity periods (UTC seconds).
const NOT_BEFORE: u64 = 1_483_225_202;
const NOT_AFTER: u64 = 2_178_918_000;
const CERTIFICATES: &[&str] = &[
    "b1bfa71ba445f0de14172a1382db19e8848f93b850eb78bd13fdabc1be7e2481",
    "5d5ad97b7ddd81a9d644ed590abe98a3206370c924cf0f78855f8b81d6b224f9",
    "3d7264910172d0efbeba5ddb6f1d00218687fb48dec0dbf14618a8d6c3dfa070",
    "79331784354dc9ed24743a5213992f16b722ebe7ae7971e249bd705947a7a9ff",
    "4dd3900914576bba84e2486e3cc7e4fed6c30f6134dfc4994ca2c25d29f6d900",
    "3e25037c63760ce47d02192e5a19ecb580d254b59ef1224761f05ef1bbda2292",
    "33bdd91a730e1ed29370131423529f0c0b9809907278c5278f7eb22e5bfb72c2",
    "337e7270e146af16d5ca83dca3f0651becd7d9f900e9a678a98c6f783b72c75e",
    "00f66b95d7bac4c96bc9928776ce9c3bd0d61965dc9f2577b6682b76df3d00c6",
    "7eddc4cf2feafe969cea5f852f8374e02abf357b9273d5d8769e8598d65219bd",
];

pub fn trusted(cert: &CertificateDer<'_>, name: &ServerName<'_>, now: UnixTime) -> bool {
    allowed_name(name)
        && (NOT_BEFORE..=NOT_AFTER).contains(&now.as_secs())
        && CERTIFICATES.contains(&format!("{:x}", Sha256::digest(cert.as_ref())).as_str())
}

fn allowed_name(name: &ServerName<'_>) -> bool {
    match name {
        ServerName::IpAddress(_) => true, // The exact certificate authenticates the IMMI identity.
        ServerName::DnsName(name) => name
            .as_ref()
            .to_ascii_lowercase()
            .strip_suffix(".immedia-semi.com")
            .is_some_and(|label| !label.is_empty() && !label.contains('.')),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trust_is_narrow_and_unknown_certificates_never_match()
    -> Result<(), Box<dyn std::error::Error>> {
        for name in ["rest-e001.immedia-semi.com", "192.0.2.1"] {
            let name = ServerName::try_from(name)?;
            assert!(allowed_name(&name));
            assert!(!trusted(
                &CertificateDer::from(vec![0; 100]),
                &name,
                UnixTime::now()
            ));
        }
        for name in [
            "evil.invalid",
            "immedia-semi.com.evil.invalid",
            "a.b.immedia-semi.com",
        ] {
            assert!(!allowed_name(&ServerName::try_from(name)?));
        }
        assert_eq!(CERTIFICATES.len(), 10);
        assert!(CERTIFICATES.iter().all(|pin| pin.len() == 64));
        Ok(())
    }
}
