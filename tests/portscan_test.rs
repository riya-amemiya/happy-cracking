use happy_cracking::crypto::portscan::{self, COMMON_PORTS, is_common_port, parse_nmap_output};

#[test]
fn common_ports_include_ftp_ssh_http() {
    assert!(is_common_port(21));
    assert!(is_common_port(22));
    assert!(is_common_port(80));
    assert!(is_common_port(443));
    assert!(!is_common_port(12345));
    assert!(COMMON_PORTS.iter().any(|(p, n)| *p == 21 && *n == "ftp"));
}

#[test]
fn parse_normal_nmap_output() {
    let text = r#"
Starting Nmap 7.94
Nmap scan report for 10.0.0.1
Host is up (0.001s latency).
PORT     STATE SERVICE
21/tcp   open  ftp
22/tcp   open  ssh
80/tcp   open  http
12345/tcp open  unknown
113/tcp  closed ident
"#;
    let ports = parse_nmap_output(text);
    assert!(ports.iter().any(|p| p.port == 21 && p.is_common));
    assert!(ports.iter().any(|p| p.port == 22 && p.service == "ssh"));
    assert!(ports.iter().any(|p| p.port == 80));
    assert!(ports.iter().any(|p| p.port == 12345 && !p.is_common));
    assert!(!ports.iter().any(|p| p.port == 113));
}

#[test]
fn parse_greppable_nmap_output() {
    let text = "Host: 127.0.0.1 ()  Status: Up\nHost: 127.0.0.1 ()  Ports: 22/open/tcp//ssh///, 80/open/tcp//http///, 9999/open/tcp//unknown///\n";
    let ports = parse_nmap_output(text);
    assert_eq!(ports.len(), 3);
    assert!(ports.iter().any(|p| p.port == 22 && p.is_common));
    assert!(ports.iter().any(|p| p.port == 80 && p.is_common));
    assert!(ports.iter().any(|p| p.port == 9999 && !p.is_common));
}

#[test]
fn parse_greppable_skips_malformed_entry_keeps_valid() {
    // Unparseable port in the middle must not discard the whole Ports line.
    let text =
        "Host: 10.0.0.1 ()  Ports: 21/open/tcp//ftp///, bad/open/tcp//x///, 22/open/tcp//ssh///\n";
    let ports = parse_nmap_output(text);
    assert!(
        ports.iter().any(|p| p.port == 21),
        "expected ftp port kept, got {:?}",
        ports
    );
    assert!(
        ports.iter().any(|p| p.port == 22),
        "expected ssh port kept, got {:?}",
        ports
    );
    assert!(!ports.iter().any(|p| p.service == "x"));
}

#[test]
fn parse_xml_style_line() {
    let text =
        r#"<port protocol="tcp" portid="443"><state state="open"/><service name="https"/></port>"#;
    let ports = parse_nmap_output(text);
    assert!(ports.iter().any(|p| p.port == 443 && p.is_common));
}

#[test]
fn empty_input_yields_no_ports() {
    assert!(parse_nmap_output("").is_empty());
    assert!(parse_nmap_output("nothing useful here").is_empty());
}

#[test]
fn common_service_name_lookup() {
    assert_eq!(portscan::common_service_name(21), Some("ftp"));
    assert_eq!(portscan::common_service_name(9999), None);
}
