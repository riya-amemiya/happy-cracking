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

#[test]
fn validate_nmap_target_accepts_host_ip_cidr_and_range() {
    for target in [
        "127.0.0.1",
        "scanme.nmap.org",
        "192.168.0.0/24",
        "10.0.0.1-254",
        "fe80::1",
        "2001:db8::/32",
        "192.168.1.1,10.0.0.1",
    ] {
        portscan::validate_nmap_target(target)
            .unwrap_or_else(|e| panic!("expected {target:?} to be accepted: {e}"));
    }
}

#[test]
fn validate_nmap_target_rejects_option_injection_and_metacharacters() {
    // Option-like strings must be rejected before they are forwarded to nmap
    // as a positional target (otherwise they are parsed as flags).
    for target in [
        "-oN",
        "-oN /tmp/out",
        "--script",
        "--privileged",
        "-iL",
        "-iL /etc/hosts",
        "host;id",
        "127.0.0.1|nmap",
        "127.0.0.1$(id)",
        "host`id`",
        "../etc/passwd",
        "",
        "   ",
        &"a".repeat(5000),
    ] {
        assert!(
            portscan::validate_nmap_target(target).is_err(),
            "expected {target:?} to be rejected"
        );
    }
}

#[test]
fn run_nmap_rejects_option_like_target_without_spawning() {
    let err = portscan::run_nmap("-oN /tmp/out", None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("target") || msg.contains("invalid") || msg.contains("'-'"),
        "validation error should mention the target, got: {msg}"
    );
}

#[test]
fn validate_nmap_extra_args_accepts_scan_options() {
    for args in [
        "",
        "-sV -T4",
        "-Pn -p 80,443 --top-ports 100",
        "-sC --open --script-args http.useragent=hc",
        "-sU -A",
    ] {
        portscan::validate_nmap_extra_args(args)
            .unwrap_or_else(|e| panic!("expected {args:?} to be accepted: {e}"));
    }
}

#[test]
fn validate_nmap_extra_args_rejects_file_and_script_flags() {
    for args in [
        "-oN /tmp/out",
        "-oX scan.xml",
        "-oG scan.gnmap",
        "-oA scan",
        "-iL /etc/hosts",
        "-iL/etc/hosts",
        "--script vuln",
        "--script=/tmp/evil.nse",
        "--script-args-file /tmp/args",
        "--datadir /tmp",
        "--excludefile /tmp/x",
        "--resume /tmp/x",
        "--stylesheet http://example.invalid/x.xsl",
        "--append-output",
        "--servicedb /tmp/services",
        "--versiondb /tmp/versions",
        &"a".repeat(5000),
    ] {
        assert!(
            portscan::validate_nmap_extra_args(args).is_err(),
            "expected {args:?} to be rejected"
        );
    }
}

#[test]
fn run_nmap_rejects_dangerous_extra_args_without_spawning() {
    let err = portscan::run_nmap("127.0.0.1", Some("-oN /tmp/out")).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("output-file") || msg.contains("extra args") || msg.contains("-o"),
        "validation error should mention extra args, got: {msg}"
    );
}
