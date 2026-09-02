use anyhow::Result;
use clap::Subcommand;
use rayon::prelude::*;

const ARMY: [&str; 5] = ["I", "II", "III", "IV", "V"];
const NAVY: [&str; 8] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII"];
const MAX_SCANS: u64 = 20_000_000;

#[derive(Subcommand)]
pub enum EnigmaAction {
    #[command(about = "Encrypt with Enigma I/M3/M4")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(
            long,
            default_value = "I II III",
            help = "Three moving rotors, or BETA|GAMMA plus three"
        )]
        rotors: String,
        #[arg(long, default_value = "B", help = "Reflector A/B/C or B-THIN/C-THIN")]
        reflector: String,
        #[arg(long, default_value = "AAA", help = "Ring settings as letters or 1-26")]
        rings: String,
        #[arg(
            long,
            default_value = "AAA",
            help = "Window positions as letters or 1-26"
        )]
        position: String,
        #[arg(long, default_value = "", help = "Plugboard pairs such as AB CD EF")]
        plugboard: String,
    },
    #[command(about = "Decrypt Enigma I/M3/M4")]
    Decrypt {
        #[arg(help = "Ciphertext")]
        input: String,
        #[arg(
            long,
            default_value = "I II III",
            help = "Three moving rotors, or BETA|GAMMA plus three"
        )]
        rotors: String,
        #[arg(long, default_value = "B", help = "Reflector A/B/C or B-THIN/C-THIN")]
        reflector: String,
        #[arg(long, default_value = "AAA", help = "Ring settings as letters or 1-26")]
        rings: String,
        #[arg(
            long,
            default_value = "AAA",
            help = "Window positions as letters or 1-26"
        )]
        position: String,
        #[arg(long, default_value = "", help = "Plugboard pairs such as AB CD EF")]
        plugboard: String,
    },
    #[command(about = "Recover settings from a crib")]
    Crack {
        #[arg(help = "Ciphertext")]
        input: String,
        #[arg(long, help = "Known plaintext fragment")]
        crib: String,
        #[arg(long, default_value = "", help = "Fix rotors, or search the pool")]
        rotors: String,
        #[arg(long, default_value = "", help = "Fix reflector, or search B and C")]
        reflector: String,
        #[arg(long, default_value = "", help = "Fix rings, or AAA / AAAA")]
        rings: String,
        #[arg(long, default_value = "", help = "Fix window positions, or search all")]
        position: String,
        #[arg(long, default_value = "", help = "Known plugboard pairs")]
        plugboard: String,
        #[arg(long, help = "Include rotors VI-VIII in the search pool")]
        navy: bool,
        #[arg(long, help = "Search M4 greek plus thin reflectors")]
        m4: bool,
        #[arg(long, help = "Also search ring settings, requires --position")]
        search_rings: bool,
        #[arg(long, help = "Only accept a crib at this letter offset")]
        offset: Option<usize>,
        #[arg(long, default_value_t = 20, help = "Stop after this many hits")]
        max_hits: usize,
    },
}

pub fn run(action: EnigmaAction) -> Result<()> {
    match action {
        EnigmaAction::Encrypt {
            input,
            rotors,
            reflector,
            rings,
            position,
            plugboard,
        }
        | EnigmaAction::Decrypt {
            input,
            rotors,
            reflector,
            rings,
            position,
            plugboard,
        } => {
            println!(
                "{}",
                transform(&input, &rotors, &reflector, &rings, &position, &plugboard)?
            );
        }
        EnigmaAction::Crack {
            input,
            crib,
            rotors,
            reflector,
            rings,
            position,
            plugboard,
            navy,
            m4,
            search_rings,
            offset,
            max_hits,
        } => {
            let hits = crack(
                &input,
                &crib,
                &rotors,
                &reflector,
                &rings,
                &position,
                &plugboard,
                navy,
                m4,
                search_rings,
                offset,
                max_hits,
            )?;
            if hits.is_empty() {
                anyhow::bail!("no settings matched the crib");
            }
            for hit in hits {
                println!(
                    "rotors={} reflector={} rings={} position={} offset={}",
                    hit.rotors, hit.reflector, hit.rings, hit.position, hit.offset
                );
                println!("{}", hit.plaintext);
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Rotor {
    name: &'static str,
    fwd: [u8; 26],
    rev: [u8; 26],
    turnovers: [bool; 26],
    ring: u8,
    pos: u8,
}

impl Rotor {
    fn new(name: &'static str, wiring: &[u8; 26], turnovers: &[u8]) -> Self {
        let mut rev = [0u8; 26];
        for (i, &w) in wiring.iter().enumerate() {
            rev[w as usize] = i as u8;
        }
        let mut marks = [false; 26];
        for &t in turnovers {
            marks[t as usize] = true;
        }
        Self {
            name,
            fwd: *wiring,
            rev,
            turnovers: marks,
            ring: 0,
            pos: 0,
        }
    }

    fn at_notch(&self) -> bool {
        self.turnovers[self.pos as usize]
    }

    fn step(&mut self) {
        self.pos = (self.pos + 1) % 26;
    }

    fn map(&self, table: &[u8; 26], c: u8) -> u8 {
        let shift = (self.pos + 26 - self.ring) % 26;
        let wired = table[((c + shift) % 26) as usize];
        (wired + 26 - shift) % 26
    }

    fn forward(&self, c: u8) -> u8 {
        self.map(&self.fwd, c)
    }

    fn inverse(&self, c: u8) -> u8 {
        self.map(&self.rev, c)
    }
}

fn wiring(text: &str) -> [u8; 26] {
    let bytes = text.as_bytes();
    let mut out = [0u8; 26];
    for i in 0..26 {
        out[i] = bytes[i] - b'A';
    }
    out
}

fn named_moving(name: &str) -> Result<Rotor> {
    match name.trim().to_ascii_uppercase().as_str() {
        "I" | "1" => Ok(Rotor::new(
            "I",
            &wiring("EKMFLGDQVZNTOWYHXUSPAIBRCJ"),
            &[b'Q' - b'A'],
        )),
        "II" | "2" => Ok(Rotor::new(
            "II",
            &wiring("AJDKSIRUXBLHWTMCQGZNPYFVOE"),
            &[b'E' - b'A'],
        )),
        "III" | "3" => Ok(Rotor::new(
            "III",
            &wiring("BDFHJLCPRTXVZNYEIWGAKMUSQO"),
            &[b'V' - b'A'],
        )),
        "IV" | "4" => Ok(Rotor::new(
            "IV",
            &wiring("ESOVPZJAYQUIRHXLNFTGKDCMWB"),
            &[b'J' - b'A'],
        )),
        "V" | "5" => Ok(Rotor::new(
            "V",
            &wiring("VZBRGITYUPSDNHLXAWMJQOFECK"),
            &[b'Z' - b'A'],
        )),
        "VI" | "6" => Ok(Rotor::new(
            "VI",
            &wiring("JPGVOUMFYQBENHZRDKASXLICTW"),
            &[b'Z' - b'A', b'M' - b'A'],
        )),
        "VII" | "7" => Ok(Rotor::new(
            "VII",
            &wiring("NZJHGRCXMYSWBOUFAIVLPEKQDT"),
            &[b'Z' - b'A', b'M' - b'A'],
        )),
        "VIII" | "8" => Ok(Rotor::new(
            "VIII",
            &wiring("FKQHTLXOCBJSPDZRAMEWNIUYGV"),
            &[b'Z' - b'A', b'M' - b'A'],
        )),
        other => anyhow::bail!("unknown rotor {other}"),
    }
}

fn named_greek(name: &str) -> Result<Rotor> {
    match name.trim().to_ascii_uppercase().as_str() {
        "BETA" | "\u{0392}" => Ok(Rotor::new(
            "BETA",
            &wiring("LEYJVCNIXWPBQMDRTAKZGFUHOS"),
            &[],
        )),
        "GAMMA" | "\u{0393}" => Ok(Rotor::new(
            "GAMMA",
            &wiring("FSOKANUERHMBTIYCWLQPZXVGJD"),
            &[],
        )),
        other => anyhow::bail!("unknown greek rotor {other}"),
    }
}

fn named_reflector(name: &str, m4: bool) -> Result<(&'static str, [u8; 26])> {
    let key = name.trim().to_ascii_uppercase();
    let key = key.as_str();
    if m4 {
        return match key {
            "B" | "B-THIN" | "BTHIN" | "THIN-B" | "THINB" | "BRUNO" | "UKW-B" | "UKWB" => {
                Ok(("B-THIN", wiring("ENKQAUYWJICOPBLMDXZVFTHRGS")))
            }
            "C" | "C-THIN" | "CTHIN" | "THIN-C" | "THINC" | "CAESAR" | "UKW-C" | "UKWC" => {
                Ok(("C-THIN", wiring("RDOBJNTKVEHMLFCWZAXGYIPSUQ")))
            }
            other => anyhow::bail!("M4 reflector must be B-THIN or C-THIN, not {other}"),
        };
    }
    match key {
        "A" => Ok(("A", wiring("EJMZALYXVBWFCRQUONTSPIKHGD"))),
        "B" => Ok(("B", wiring("YRUHQSLDPXNGOKMIEBFZCWVJAT"))),
        "C" => Ok(("C", wiring("FVPJIAOYEDRZXWGCTKUQSBNMHL"))),
        other => anyhow::bail!("unknown reflector {other}"),
    }
}

fn tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect()
}

fn parse_setting(token: &str) -> Result<u8> {
    let t = token.trim();
    if t.len() == 1 {
        let b = t.as_bytes()[0];
        if b.is_ascii_alphabetic() {
            return Ok(b.to_ascii_uppercase() - b'A');
        }
    }
    let n: u32 = t
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid setting {t}"))?;
    if (1..=26).contains(&n) {
        Ok((n - 1) as u8)
    } else {
        anyhow::bail!("setting must be A-Z or 1-26");
    }
}

fn parse_letters(text: &str, greek: bool) -> Result<[u8; 4]> {
    let parts = tokens(text);
    if parts.len() == 1 && parts[0].bytes().all(|b| b.is_ascii_alphabetic()) {
        let b = parts[0].as_bytes();
        if greek && b.len() == 3 {
            return Ok([
                0,
                b[0].to_ascii_uppercase() - b'A',
                b[1].to_ascii_uppercase() - b'A',
                b[2].to_ascii_uppercase() - b'A',
            ]);
        }
        if greek && b.len() == 4 {
            return Ok([
                b[0].to_ascii_uppercase() - b'A',
                b[1].to_ascii_uppercase() - b'A',
                b[2].to_ascii_uppercase() - b'A',
                b[3].to_ascii_uppercase() - b'A',
            ]);
        }
        if !greek && b.len() == 3 {
            return Ok([
                0,
                b[0].to_ascii_uppercase() - b'A',
                b[1].to_ascii_uppercase() - b'A',
                b[2].to_ascii_uppercase() - b'A',
            ]);
        }
    }
    let want = if greek { 4 } else { 3 };
    if greek && parts.len() == 3 {
        return Ok([
            0,
            parse_setting(parts[0])?,
            parse_setting(parts[1])?,
            parse_setting(parts[2])?,
        ]);
    }
    if parts.len() != want {
        anyhow::bail!("expected {want} setting values");
    }
    if greek {
        Ok([
            parse_setting(parts[0])?,
            parse_setting(parts[1])?,
            parse_setting(parts[2])?,
            parse_setting(parts[3])?,
        ])
    } else {
        Ok([
            0,
            parse_setting(parts[0])?,
            parse_setting(parts[1])?,
            parse_setting(parts[2])?,
        ])
    }
}

fn parse_plugboard(text: &str) -> Result<[u8; 26]> {
    let mut map = [0u8; 26];
    for (i, slot) in map.iter_mut().enumerate() {
        *slot = i as u8;
    }
    let compact: String = text
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if !compact.is_empty()
        && compact.len().is_multiple_of(2)
        && tokens(text).iter().all(|t| t.len() != 2)
    {
        pair_plugboard(&mut map, compact.as_bytes())?;
        return Ok(map);
    }
    for tok in tokens(text) {
        let letters: String = tok
            .chars()
            .filter(char::is_ascii_alphabetic)
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if letters.len() != 2 {
            anyhow::bail!("plugboard pair must be two letters");
        }
        pair_plugboard(&mut map, letters.as_bytes())?;
    }
    Ok(map)
}

fn pair_plugboard(map: &mut [u8; 26], letters: &[u8]) -> Result<()> {
    if !letters.len().is_multiple_of(2) {
        anyhow::bail!("plugboard needs an even number of letters");
    }
    if letters.len() / 2 > 13 {
        anyhow::bail!("plugboard allows at most 13 pairs");
    }
    for chunk in letters.chunks(2) {
        let a = chunk[0] - b'A';
        let b = chunk[1] - b'A';
        if a == b {
            anyhow::bail!("plugboard cannot pair a letter with itself");
        }
        if map[a as usize] != a || map[b as usize] != b {
            anyhow::bail!("plugboard letter used twice");
        }
        map[a as usize] = b;
        map[b as usize] = a;
    }
    Ok(())
}

fn parse_wheel_names(text: &str) -> Result<(Option<Rotor>, [Rotor; 3])> {
    let parts = tokens(text);
    if parts.len() == 3 {
        let a = parts[0].trim().to_ascii_uppercase();
        let b = parts[1].trim().to_ascii_uppercase();
        let c = parts[2].trim().to_ascii_uppercase();
        if a == b || a == c || b == c {
            anyhow::bail!("rotors must be distinct");
        }
        return Ok((
            None,
            [named_moving(&a)?, named_moving(&b)?, named_moving(&c)?],
        ));
    }
    if parts.len() == 4 {
        let g = parts[0].trim().to_ascii_uppercase();
        let a = parts[1].trim().to_ascii_uppercase();
        let b = parts[2].trim().to_ascii_uppercase();
        let c = parts[3].trim().to_ascii_uppercase();
        if a == b || a == c || b == c {
            anyhow::bail!("rotors must be distinct");
        }
        return Ok((
            Some(named_greek(&g)?),
            [named_moving(&a)?, named_moving(&b)?, named_moving(&c)?],
        ));
    }
    anyhow::bail!("Enigma needs three moving rotors, or BETA|GAMMA plus three");
}

struct Machine {
    greek: Option<Rotor>,
    left: Rotor,
    mid: Rotor,
    right: Rotor,
    reflector: [u8; 26],
    plug: [u8; 26],
}

impl Machine {
    fn step(&mut self) {
        let mid_notch = self.mid.at_notch();
        let right_notch = self.right.at_notch();
        if mid_notch {
            self.left.step();
            self.mid.step();
        } else if right_notch {
            self.mid.step();
        }
        self.right.step();
    }

    fn encode(&mut self, c: u8) -> u8 {
        self.step();
        let mut x = self.plug[c as usize];
        x = self.right.forward(x);
        x = self.mid.forward(x);
        x = self.left.forward(x);
        if let Some(g) = &self.greek {
            x = g.forward(x);
        }
        x = self.reflector[x as usize];
        if let Some(g) = &self.greek {
            x = g.inverse(x);
        }
        x = self.left.inverse(x);
        x = self.mid.inverse(x);
        x = self.right.inverse(x);
        self.plug[x as usize]
    }
}

fn letters_label(vals: &[u8]) -> String {
    vals.iter().map(|&v| (b'A' + v) as char).collect()
}

fn assemble(
    mut greek: Option<Rotor>,
    mut moving: [Rotor; 3],
    reflector: [u8; 26],
    rings: [u8; 4],
    pos: [u8; 4],
    plug: [u8; 26],
) -> Machine {
    if let Some(g) = greek.as_mut() {
        g.ring = rings[0];
        g.pos = pos[0];
    }
    moving[0].ring = rings[1];
    moving[1].ring = rings[2];
    moving[2].ring = rings[3];
    moving[0].pos = pos[1];
    moving[1].pos = pos[2];
    moving[2].pos = pos[3];
    let [left, mid, right] = moving;
    Machine {
        greek,
        left,
        mid,
        right,
        reflector,
        plug,
    }
}

fn run_machine(input: &str, mut machine: Machine) -> Result<String> {
    let mut bytes = input.as_bytes().to_vec();
    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let upper = b.is_ascii_uppercase();
            let enc = machine.encode(b.to_ascii_uppercase() - b'A');
            *b = enc + if upper { b'A' } else { b'a' };
        }
    }
    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("enigma produced invalid UTF-8"))
}

pub fn transform(
    input: &str,
    rotors: &str,
    reflector: &str,
    rings: &str,
    position: &str,
    plugboard: &str,
) -> Result<String> {
    let (greek, moving) = parse_wheel_names(rotors)?;
    let m4 = greek.is_some();
    let (_, ukw) = named_reflector(reflector, m4)?;
    let ring = parse_letters(rings, m4)?;
    let pos = parse_letters(position, m4)?;
    let plug = parse_plugboard(plugboard)?;
    run_machine(input, assemble(greek, moving, ukw, ring, pos, plug))
}

pub fn encrypt(
    input: &str,
    rotors: &str,
    reflector: &str,
    rings: &str,
    position: &str,
    plugboard: &str,
) -> Result<String> {
    transform(input, rotors, reflector, rings, position, plugboard)
}

pub fn decrypt(
    input: &str,
    rotors: &str,
    reflector: &str,
    rings: &str,
    position: &str,
    plugboard: &str,
) -> Result<String> {
    transform(input, rotors, reflector, rings, position, plugboard)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub rotors: String,
    pub reflector: String,
    pub rings: String,
    pub position: String,
    pub offset: usize,
    pub plaintext: String,
}

fn alpha_bytes(text: &str) -> Vec<u8> {
    text.bytes()
        .filter(u8::is_ascii_alphabetic)
        .map(|b| b.to_ascii_uppercase() - b'A')
        .collect()
}

fn find_crib(plain: &[u8], crib: &[u8]) -> Option<usize> {
    if crib.is_empty() || plain.len() < crib.len() {
        return None;
    }
    plain.windows(crib.len()).position(|w| w == crib)
}

fn wheel_label(greek: Option<&Rotor>, moving: &[Rotor; 3]) -> String {
    match greek {
        Some(g) => format!(
            "{} {} {} {}",
            g.name, moving[0].name, moving[1].name, moving[2].name
        ),
        None => format!("{} {} {}", moving[0].name, moving[1].name, moving[2].name),
    }
}

fn ring_label(m4: bool, rings: [u8; 4]) -> String {
    if m4 {
        letters_label(&rings)
    } else {
        letters_label(&rings[1..])
    }
}

fn moving_perms(pool: &[&'static str]) -> Vec<[&'static str; 3]> {
    let mut out = Vec::new();
    for i in 0..pool.len() {
        for j in 0..pool.len() {
            if j == i {
                continue;
            }
            for k in 0..pool.len() {
                if k == i || k == j {
                    continue;
                }
                out.push([pool[i], pool[j], pool[k]]);
            }
        }
    }
    out
}

struct Job {
    greek_name: Option<&'static str>,
    moving_names: [&'static str; 3],
    reflector_name: &'static str,
    rings: [u8; 4],
}

fn decode_letters(cipher: &[u8], mut machine: Machine) -> Vec<u8> {
    cipher.iter().map(|&c| machine.encode(c)).collect()
}

fn scan_job(
    job: &Job,
    cipher: &[u8],
    crib: &[u8],
    plug: [u8; 26],
    offset: Option<usize>,
) -> Vec<Hit> {
    let m4 = job.greek_name.is_some();
    let greek = job.greek_name.map(|n| named_greek(n).expect("greek"));
    let moving = [
        named_moving(job.moving_names[0]).expect("rotor"),
        named_moving(job.moving_names[1]).expect("rotor"),
        named_moving(job.moving_names[2]).expect("rotor"),
    ];
    let (_, ukw) = named_reflector(job.reflector_name, m4).expect("ukw");
    let mut hits = Vec::new();
    let pos_span = if m4 { 26 * 26 * 26 * 26 } else { 26 * 26 * 26 };
    for raw in 0..pos_span {
        let pos = if m4 {
            [
                (raw / (26 * 26 * 26)) as u8,
                ((raw / (26 * 26)) % 26) as u8,
                ((raw / 26) % 26) as u8,
                (raw % 26) as u8,
            ]
        } else {
            [
                0,
                (raw / (26 * 26)) as u8,
                ((raw / 26) % 26) as u8,
                (raw % 26) as u8,
            ]
        };
        let machine = assemble(greek.clone(), moving.clone(), ukw, job.rings, pos, plug);
        let decoded = decode_letters(cipher, machine);
        let Some(at) = find_crib(&decoded, crib) else {
            continue;
        };
        if offset.is_some_and(|want| want != at) {
            continue;
        }
        let mut out = String::with_capacity(decoded.len());
        for &p in &decoded {
            out.push((b'A' + p) as char);
        }
        hits.push(Hit {
            rotors: wheel_label(greek.as_ref(), &moving),
            reflector: job.reflector_name.to_string(),
            rings: ring_label(m4, job.rings),
            position: ring_label(m4, pos),
            offset: at,
            plaintext: out,
        });
    }
    hits
}

#[allow(clippy::too_many_arguments)]
pub fn crack(
    input: &str,
    crib: &str,
    rotors: &str,
    reflector: &str,
    rings: &str,
    position: &str,
    plugboard: &str,
    navy: bool,
    m4: bool,
    search_rings: bool,
    offset: Option<usize>,
    max_hits: usize,
) -> Result<Vec<Hit>> {
    let cipher = alpha_bytes(input);
    let needle = alpha_bytes(crib);
    if needle.is_empty() {
        anyhow::bail!("crib must contain a letter");
    }
    if cipher.len() < needle.len() {
        anyhow::bail!("ciphertext is shorter than the crib");
    }
    if max_hits == 0 {
        return Ok(Vec::new());
    }
    let plug = parse_plugboard(plugboard)?;
    let fixed_wheels = if rotors.trim().is_empty() {
        None
    } else {
        Some(parse_wheel_names(rotors)?)
    };
    let want_m4 = m4 || fixed_wheels.as_ref().is_some_and(|(g, _)| g.is_some());
    if want_m4 && fixed_wheels.is_none() {
        anyhow::bail!("M4 search needs --rotors");
    }
    if search_rings && position.trim().is_empty() {
        anyhow::bail!("--search-rings needs --position");
    }

    let mut jobs = Vec::new();
    let pool = if navy { &NAVY[..] } else { &ARMY[..] };
    let moving_sets: Vec<[&'static str; 3]> = if let Some((_, ref mv)) = fixed_wheels {
        vec![[mv[0].name, mv[1].name, mv[2].name]]
    } else {
        moving_perms(pool)
    };
    let greek_sets: Vec<Option<&'static str>> = if let Some((ref g, _)) = fixed_wheels {
        vec![g.as_ref().map(|r| r.name)]
    } else {
        vec![None]
    };
    let reflectors: Vec<&'static str> = if !reflector.trim().is_empty() {
        let (name, _) = named_reflector(reflector, want_m4)?;
        vec![name]
    } else if want_m4 {
        vec!["B-THIN", "C-THIN"]
    } else {
        vec!["B", "C"]
    };

    let ring_grid: Vec<[u8; 4]> = if search_rings {
        let mut all = Vec::with_capacity(26 * 26 * 26);
        for a in 0..26u8 {
            for b in 0..26 {
                for c in 0..26 {
                    all.push([0, a, b, c]);
                }
            }
        }
        all
    } else if rings.trim().is_empty() {
        vec![[0, 0, 0, 0]]
    } else {
        vec![parse_letters(rings, want_m4)?]
    };

    if !position.trim().is_empty() {
        let pos = parse_letters(position, want_m4)?;
        for g in &greek_sets {
            for mv in &moving_sets {
                for ukw in &reflectors {
                    for &ring in &ring_grid {
                        let greek = match g {
                            Some(name) => Some(named_greek(name)?),
                            None => None,
                        };
                        let moving = [
                            named_moving(mv[0])?,
                            named_moving(mv[1])?,
                            named_moving(mv[2])?,
                        ];
                        let (_, wiring) = named_reflector(ukw, want_m4)?;
                        let machine =
                            assemble(greek.clone(), moving.clone(), wiring, ring, pos, plug);
                        let decoded = decode_letters(&cipher, machine);
                        if let Some(at) = find_crib(&decoded, &needle)
                            && offset.is_none_or(|want| want == at)
                        {
                            let mut out = String::new();
                            for &p in &decoded {
                                out.push((b'A' + p) as char);
                            }
                            return Ok(vec![Hit {
                                rotors: wheel_label(greek.as_ref(), &moving),
                                reflector: (*ukw).to_string(),
                                rings: ring_label(want_m4, ring),
                                position: ring_label(want_m4, pos),
                                offset: at,
                                plaintext: out,
                            }]);
                        }
                    }
                }
            }
        }
        return Ok(Vec::new());
    }

    for g in &greek_sets {
        for mv in &moving_sets {
            for ukw in &reflectors {
                for &ring in &ring_grid {
                    jobs.push(Job {
                        greek_name: *g,
                        moving_names: *mv,
                        reflector_name: ukw,
                        rings: ring,
                    });
                }
            }
        }
    }

    let pos_span = if want_m4 {
        26u64 * 26 * 26 * 26
    } else {
        26 * 26 * 26
    };
    let scans = jobs.len() as u64 * pos_span;
    if scans > MAX_SCANS {
        anyhow::bail!("search space {scans} exceeds {MAX_SCANS}");
    }

    let found: Vec<Hit> = jobs
        .par_iter()
        .flat_map(|job| scan_job(job, &cipher, &needle, plug, offset))
        .collect();
    let mut found = found;
    found.sort_by(|a, b| {
        a.offset
            .cmp(&b.offset)
            .then(a.rotors.cmp(&b.rotors))
            .then(a.position.cmp(&b.position))
    });
    found.truncate(max_hits);
    Ok(found)
}
