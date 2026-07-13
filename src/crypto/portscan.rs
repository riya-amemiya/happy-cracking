use anyhow::{Context, Result};
use clap::Subcommand;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

/// Well-known ports that CTF / recon workflows usually care about first.
pub const COMMON_PORTS: &[(u16, &str)] = &[
    (21, "ftp"),
    (22, "ssh"),
    (23, "telnet"),
    (25, "smtp"),
    (53, "dns"),
    (80, "http"),
    (110, "pop3"),
    (111, "rpcbind"),
    (135, "msrpc"),
    (139, "netbios-ssn"),
    (143, "imap"),
    (443, "https"),
    (445, "microsoft-ds"),
    (993, "imaps"),
    (995, "pop3s"),
    (1433, "mssql"),
    (1521, "oracle"),
    (1723, "pptp"),
    (2049, "nfs"),
    (3306, "mysql"),
    (3389, "rdp"),
    (5432, "postgresql"),
    (5900, "vnc"),
    (6379, "redis"),
    (8080, "http-proxy"),
    (8443, "https-alt"),
    (27017, "mongodb"),
];

#[derive(Subcommand)]
pub enum PortscanAction {
    #[command(about = "Parse nmap output and list open common ports")]
    Parse {
        #[arg(help = "Path to nmap output file (-oN / -oG / -oX), or '-' for stdin")]
        file: String,
        #[arg(long, help = "Show all open ports, not only the common set")]
        all: bool,
    },
    #[command(about = "Run nmap against a target and highlight open common ports")]
    Scan {
        #[arg(help = "Target host or CIDR")]
        target: String,
        #[arg(
            short,
            long,
            help = "Extra nmap arguments as a single string (e.g. '-sV -T4')"
        )]
        args: Option<String>,
        #[arg(long, help = "Show all open ports, not only the common set")]
        all: bool,
    },
    #[command(about = "Print the built-in common-port list")]
    Commons {},
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPort {
    pub port: u16,
    pub protocol: String,
    pub service: String,
    pub state: String,
    pub is_common: bool,
}

pub fn run(action: PortscanAction) -> Result<()> {
    match action {
        PortscanAction::Parse { file, all } => {
            let text = if file == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .context("Failed to read stdin")?;
                buf
            } else {
                std::fs::read_to_string(PathBuf::from(&file))
                    .with_context(|| format!("Failed to read {}", file))?
            };
            let ports = parse_nmap_output(&text);
            print_ports(&ports, all);
        }
        PortscanAction::Scan { target, args, all } => {
            let output = run_nmap(&target, args.as_deref())?;
            print!("{}", output);
            println!();
            println!("=== Common open ports ===");
            let ports = parse_nmap_output(&output);
            print_ports(&ports, all);
        }
        PortscanAction::Commons {} => {
            for &(port, name) in COMMON_PORTS {
                println!("{:>5}/tcp  {}", port, name);
            }
        }
    }
    Ok(())
}

pub fn is_common_port(port: u16) -> bool {
    COMMON_PORTS.iter().any(|(p, _)| *p == port)
}

pub fn common_service_name(port: u16) -> Option<&'static str> {
    COMMON_PORTS
        .iter()
        .find(|(p, _)| *p == port)
        .map(|(_, name)| *name)
}

pub fn run_nmap(target: &str, extra_args: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("nmap");
    // Default to a fast common-port focused scan; user may override via --args.
    if let Some(extra) = extra_args {
        for tok in extra.split_whitespace() {
            cmd.arg(tok);
        }
    } else {
        cmd.arg("-sT");
        cmd.arg("--top-ports");
        cmd.arg("100");
    }
    cmd.arg(target);
    let output = cmd
        .output()
        .context("Failed to execute nmap (is it installed and on PATH?)")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("nmap exited with {}: {}", output.status, stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse normal (-oN), greppable (-oG), or XML (-oX) nmap output.
pub fn parse_nmap_output(text: &str) -> Vec<OpenPort> {
    let mut by_port: BTreeMap<(u16, String), OpenPort> = BTreeMap::new();

    for line in text.lines() {
        if let Some(p) = parse_normal_line(line) {
            by_port.insert((p.port, p.protocol.clone()), p);
        } else if let Some(ports) = parse_greppable_line(line) {
            for p in ports {
                by_port.insert((p.port, p.protocol.clone()), p);
            }
        } else if let Some(p) = parse_xml_port(line) {
            by_port.insert((p.port, p.protocol.clone()), p);
        }
    }

    by_port.into_values().collect()
}

fn parse_normal_line(line: &str) -> Option<OpenPort> {
    // e.g. "21/tcp   open  ftp"
    // e.g. "80/tcp open  http    syn-ack"
    let line = line.trim();
    let mut parts = line.split_whitespace();
    let port_proto = parts.next()?;
    let state = parts.next()?.to_ascii_lowercase();
    if state != "open" && state != "open|filtered" {
        return None;
    }
    let (port_s, proto) = port_proto.split_once('/')?;
    let port: u16 = port_s.parse().ok()?;
    if !proto.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let service = parts.next().unwrap_or("-").to_string();
    Some(OpenPort {
        port,
        protocol: proto.to_ascii_lowercase(),
        service,
        state,
        is_common: is_common_port(port),
    })
}

fn parse_greppable_line(line: &str) -> Option<Vec<OpenPort>> {
    // Host: 127.0.0.1 ()  Ports: 22/open/tcp//ssh///, 80/open/tcp//http///
    if !line.starts_with("Host:") {
        return None;
    }
    let ports_idx = line.find("Ports:")?;
    let ports_part = &line[ports_idx + "Ports:".len()..];
    let mut out = Vec::new();
    for entry in ports_part.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let fields: Vec<&str> = entry.split('/').collect();
        // port/state/proto/owner/service/sunrpc/rpc-info
        if fields.len() < 3 {
            continue;
        }
        let port: u16 = match fields[0].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let state = fields[1].to_ascii_lowercase();
        if state != "open" && !state.starts_with("open") {
            continue;
        }
        let protocol = fields[2].to_ascii_lowercase();
        let service = fields.get(4).copied().unwrap_or("-").to_string();
        if service.is_empty() {
            // keep "-"
        }
        out.push(OpenPort {
            port,
            protocol,
            service: if service.is_empty() {
                "-".to_string()
            } else {
                service
            },
            state,
            is_common: is_common_port(port),
        });
    }
    if out.is_empty() { None } else { Some(out) }
}

fn parse_xml_port(line: &str) -> Option<OpenPort> {
    // Lightweight line-oriented XML scrape for <port protocol="tcp" portid="22"> ... <state state="open"/>
    // Works when pretty-printed with state on nearby lines; also handles single-line forms.
    if !line.contains("<port ") && !line.contains("portid=") {
        return None;
    }
    let port = extract_xml_attr(line, "portid")?.parse::<u16>().ok()?;
    let protocol = extract_xml_attr(line, "protocol")
        .unwrap_or("tcp")
        .to_ascii_lowercase();
    let state = extract_xml_attr(line, "state")
        .unwrap_or("open")
        .to_ascii_lowercase();
    if state != "open" && !state.starts_with("open") {
        // state may be on a following line; if this line has portid only, assume open when <state is absent
        if line.contains("state=") {
            return None;
        }
    }
    let service = extract_xml_attr(line, "name").unwrap_or("-").to_string();
    let final_state = if line.contains("state=") {
        state
    } else {
        "open".to_string()
    };
    if final_state != "open" && !final_state.starts_with("open") {
        return None;
    }
    Some(OpenPort {
        port,
        protocol,
        service,
        state: final_state,
        is_common: is_common_port(port),
    })
}

fn extract_xml_attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{}=\"", name);
    let start = line.find(&key)? + key.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn print_ports(ports: &[OpenPort], show_all: bool) {
    let filtered: Vec<&OpenPort> = if show_all {
        ports.iter().collect()
    } else {
        ports.iter().filter(|p| p.is_common).collect()
    };

    if filtered.is_empty() {
        if show_all {
            println!("No open ports found in input");
        } else {
            println!("No open common ports found (use --all to list every open port)");
            let uncommon: BTreeSet<u16> = ports
                .iter()
                .filter(|p| !p.is_common)
                .map(|p| p.port)
                .collect();
            if !uncommon.is_empty() {
                println!(
                    "Other open ports present: {}",
                    uncommon
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        return;
    }

    println!("{:>5}  {:<8}  {:<10}  SERVICE", "PORT", "PROTO", "STATE");
    for p in filtered {
        let known = common_service_name(p.port).unwrap_or("");
        let service = if p.service == "-" && !known.is_empty() {
            known.to_string()
        } else if !known.is_empty() && p.service != known {
            format!("{} ({})", p.service, known)
        } else {
            p.service.clone()
        };
        let mark = if p.is_common { " *" } else { "" };
        println!(
            "{:>5}  {:<8}  {:<10}  {}{}",
            p.port, p.protocol, p.state, service, mark
        );
    }
    if !show_all {
        println!();
        println!("* = well-known / common port");
    }
}
