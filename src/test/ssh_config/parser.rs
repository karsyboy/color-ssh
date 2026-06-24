use super::parse_ssh_config;
use crate::ssh_config::ConnectionProtocol;
use crate::test::support::fs::TestWorkspace;

#[test]
fn parser_filters_hidden_and_wildcard_hosts_from_metadata() {
    let workspace = TestWorkspace::new("ssh_config", "metadata").expect("temp workspace");
    let config_path = workspace.join("config");

    workspace
        .write(
            &config_path,
            "Host app\n#_Desc Production app\n#_Profile prod\n#_pass test_pass\nHostName 10.0.0.10\nUser deploy\nPort 2200\n\nHost hidden-node\n#_hidden true\nHostName 10.0.0.20\n\nHost web-*\nHostName 10.0.0.30\n",
        )
        .expect("write config");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].name, "app");
    assert_eq!(hosts[0].protocol, ConnectionProtocol::Ssh);
    assert_eq!(hosts[0].profile.as_deref(), Some("prod"));
}

#[test]
fn parser_resolves_glob_includes_once_in_stable_order() {
    let workspace = TestWorkspace::new("ssh_config", "include_cycle").expect("temp workspace");
    let config_path = workspace.join("config");

    workspace
        .write(&config_path, "Host root\nHostName root.example\nInclude conf.d/*.conf\n")
        .expect("write root config");
    workspace
        .write_rel("conf.d/20-b.conf", "Host b\nHostName b.example\n")
        .expect("write b include");
    workspace
        .write_rel(
            "conf.d/10-a.conf",
            format!("Host a\nHostName a.example\nInclude {}\n", config_path.display()).as_str(),
        )
        .expect("write a include");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    let names: Vec<&str> = hosts.iter().map(|host| host.name.as_str()).collect();
    assert_eq!(names, vec!["root", "a", "b"]);
}

#[test]
fn parser_multi_alias_and_invalid_pass_key_behavior() {
    let workspace = TestWorkspace::new("ssh_config", "alias_pass").expect("temp workspace");
    let config_path = workspace.join("config");

    workspace
        .write(
            &config_path,
            "Host app-a app-b app-c\n#_pass shared_key\nHostName app.internal\n\nHost invalid\n#_pass ../secret\nHostName invalid.example\n",
        )
        .expect("write config");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    assert!(hosts.iter().any(|host| host.name == "app-a" && host.pass_key.as_deref() == Some("shared_key")));
    assert!(hosts.iter().any(|host| host.name == "invalid" && host.pass_key.is_none()));
}

#[test]
fn parser_handles_rdp_hosts_and_repeated_options() {
    let workspace = TestWorkspace::new("ssh_config", "rdp_repeated").expect("temp workspace");
    let config_path = workspace.join("config");

    workspace
        .write(
            &config_path,
            "Host desktop01\n#_Protocol rdp\n#_RdpDomain ACME\nHostName rdp.internal\n\nHost repeated\nHostName repeated.example\nIdentityFile ~/.ssh/id_first\nIdentityFile ~/.ssh/id_second\nCertificateFile ~/.ssh/id_first-cert.pub\nCertificateFile ~/.ssh/id_second-cert.pub\n",
        )
        .expect("write config");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    let desktop = hosts.iter().find(|host| host.name == "desktop01").expect("desktop01");
    assert_eq!(desktop.protocol, ConnectionProtocol::Rdp);

    let repeated = hosts.iter().find(|host| host.name == "repeated").expect("repeated");
    assert_eq!(repeated.identity_files.len(), 2);
    assert_eq!(repeated.other_options.get("certificatefile").map(Vec::len), Some(2));
}
