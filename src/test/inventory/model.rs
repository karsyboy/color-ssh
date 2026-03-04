use crate::inventory::ConnectionProtocol;
use std::str::FromStr;

#[test]
fn connection_protocol_from_str_known_and_unknown_variants_parse_expected_values() {
    let cases = [
        ("", ConnectionProtocol::Ssh),
        ("ssh", ConnectionProtocol::Ssh),
        ("RDP", ConnectionProtocol::Rdp),
        ("Telnet", ConnectionProtocol::Other("telnet".to_string())),
    ];

    for (input, expected) in cases {
        assert_eq!(ConnectionProtocol::from_str(input).expect("parse protocol"), expected);
    }
}

#[test]
fn connection_protocol_display_variants_render_expected_strings() {
    assert_eq!(ConnectionProtocol::Ssh.to_string(), "ssh");
    assert_eq!(ConnectionProtocol::Rdp.to_string(), "rdp");
    assert_eq!(ConnectionProtocol::Other("mosh".to_string()).to_string(), "mosh");
}
