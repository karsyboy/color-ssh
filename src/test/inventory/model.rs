use crate::inventory::ConnectionProtocol;
use std::str::FromStr;

#[test]
fn parses_connection_protocol_variants() {
    assert_eq!(ConnectionProtocol::from_str("").unwrap(), ConnectionProtocol::Ssh);
    assert_eq!(ConnectionProtocol::from_str("ssh").unwrap(), ConnectionProtocol::Ssh);
    assert_eq!(ConnectionProtocol::from_str("RDP").unwrap(), ConnectionProtocol::Rdp);
    assert_eq!(ConnectionProtocol::from_str("Telnet").unwrap(), ConnectionProtocol::Other("telnet".to_string()));
}

#[test]
fn displays_connection_protocol_variants() {
    assert_eq!(ConnectionProtocol::Ssh.to_string(), "ssh");
    assert_eq!(ConnectionProtocol::Rdp.to_string(), "rdp");
    assert_eq!(ConnectionProtocol::Other("mosh".to_string()).to_string(), "mosh");
}
