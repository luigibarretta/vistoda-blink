use crate::live::{Target, auth_header};

#[test]
fn auth_packet_is_exact_and_address_is_bounded() {
    let Ok(target) = Target::parse("immis://example.invalid:443/session__x?client_id=7") else {
        panic!("valid fixture was rejected");
    };
    let packet = auth_header(target.client_id, &target.connection_id);
    assert_eq!(packet.len(), 122);
    assert_eq!(&packet[..4], &[0, 0, 0, 0x28]);
    assert_eq!(&packet[4..24], &[0; 20]);
    assert_eq!(&packet[24..28], &7_u32.to_be_bytes());
    assert_eq!(&packet[30..98], &[0; 68]);
    assert_eq!(&packet[98..102], &16_u32.to_be_bytes());
    assert!(Target::parse("https://example.invalid:443/x?client_id=7").is_err());
}
