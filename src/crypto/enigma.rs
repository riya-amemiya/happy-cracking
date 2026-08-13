use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum EnigmaAction {
    #[command(about = "Encrypt with Enigma I/M3")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(long, default_value = "I II III", help = "Three rotors left to right")]
        rotors: String,
        #[arg(long, default_value = "B", help = "Reflector A, B, or C")]
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
    #[command(about = "Decrypt Enigma I/M3")]
    Decrypt {
        #[arg(help = "Ciphertext")]
        input: String,
        #[arg(long, default_value = "I II III", help = "Three rotors left to right")]
        rotors: String,
        #[arg(long, default_value = "B", help = "Reflector A, B, or C")]
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
    }
    Ok(())
}

struct Rotor {
    fwd: [u8; 26],
    rev: [u8; 26],
    turnovers: [bool; 26],
    ring: u8,
    pos: u8,
}

impl Rotor {
    fn new(wiring: &[u8; 26], turnovers: &[u8]) -> Self {
        let mut rev = [0u8; 26];
        for (i, &w) in wiring.iter().enumerate() {
            rev[w as usize] = i as u8;
        }
        let mut marks = [false; 26];
        for &t in turnovers {
            marks[t as usize] = true;
        }
        Self {
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

fn named_rotor(name: &str) -> Result<Rotor> {
    match name.trim().to_ascii_uppercase().as_str() {
        "I" | "1" => Ok(Rotor::new(
            &wiring("EKMFLGDQVZNTOWYHXUSPAIBRCJ"),
            &[b'Q' - b'A'],
        )),
        "II" | "2" => Ok(Rotor::new(
            &wiring("AJDKSIRUXBLHWTMCQGZNPYFVOE"),
            &[b'E' - b'A'],
        )),
        "III" | "3" => Ok(Rotor::new(
            &wiring("BDFHJLCPRTXVZNYEIWGAKMUSQO"),
            &[b'V' - b'A'],
        )),
        "IV" | "4" => Ok(Rotor::new(
            &wiring("ESOVPZJAYQUIRHXLNFTGKDCMWB"),
            &[b'J' - b'A'],
        )),
        "V" | "5" => Ok(Rotor::new(
            &wiring("VZBRGITYUPSDNHLXAWMJQOFECK"),
            &[b'Z' - b'A'],
        )),
        "VI" | "6" => Ok(Rotor::new(
            &wiring("JPGVOUMFYQBENHZRDKASXLICTW"),
            &[b'Z' - b'A', b'M' - b'A'],
        )),
        "VII" | "7" => Ok(Rotor::new(
            &wiring("NZJHGRCXMYSWBOUFAIVLPEKQDT"),
            &[b'Z' - b'A', b'M' - b'A'],
        )),
        "VIII" | "8" => Ok(Rotor::new(
            &wiring("FKQHTLXOCBJSPDZRAMEWNIUYGV"),
            &[b'Z' - b'A', b'M' - b'A'],
        )),
        other => anyhow::bail!("unknown rotor {other}"),
    }
}

fn named_reflector(name: &str) -> Result<[u8; 26]> {
    match name.trim().to_ascii_uppercase().as_str() {
        "A" => Ok(wiring("EJMZALYXVBWFCRQUONTSPIKHGD")),
        "B" => Ok(wiring("YRUHQSLDPXNGOKMIEBFZCWVJAT")),
        "C" => Ok(wiring("FVPJIAOYEDRZXWGCTKUQSBNMHL")),
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
            return Ok((b.to_ascii_uppercase() - b'A') % 26);
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

fn parse_three(text: &str, what: &str) -> Result<[u8; 3]> {
    let parts = tokens(text);
    if parts.len() == 1 && parts[0].len() == 3 && parts[0].bytes().all(|b| b.is_ascii_alphabetic())
    {
        let b = parts[0].as_bytes();
        return Ok([
            b[0].to_ascii_uppercase() - b'A',
            b[1].to_ascii_uppercase() - b'A',
            b[2].to_ascii_uppercase() - b'A',
        ]);
    }
    if parts.len() != 3 {
        anyhow::bail!("{what} needs three values");
    }
    Ok([
        parse_setting(parts[0])?,
        parse_setting(parts[1])?,
        parse_setting(parts[2])?,
    ])
}

fn parse_plugboard(text: &str) -> Result<[u8; 26]> {
    let mut map = [0u8; 26];
    for (i, slot) in map.iter_mut().enumerate() {
        *slot = i as u8;
    }
    let compact: String = text
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if !compact.is_empty()
        && compact.len().is_multiple_of(2)
        && tokens(text).iter().all(|t| t.len() != 2)
    {
        return pair_plugboard(&mut map, compact.as_bytes());
    }
    for tok in tokens(text) {
        let letters: String = tok
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if letters.len() != 2 {
            anyhow::bail!("plugboard pair must be two letters");
        }
        pair_plugboard(&mut map, letters.as_bytes())?;
    }
    Ok(map)
}

fn pair_plugboard(map: &mut [u8; 26], letters: &[u8]) -> Result<[u8; 26]> {
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
    Ok(*map)
}

fn parse_rotors(text: &str) -> Result<[Rotor; 3]> {
    let parts = tokens(text);
    if parts.len() != 3 {
        anyhow::bail!("Enigma I/M3 needs three rotors");
    }
    let a = parts[0].trim().to_ascii_uppercase();
    let b = parts[1].trim().to_ascii_uppercase();
    let c = parts[2].trim().to_ascii_uppercase();
    if a == b || a == c || b == c {
        anyhow::bail!("rotors must be distinct");
    }
    Ok([named_rotor(&a)?, named_rotor(&b)?, named_rotor(&c)?])
}

struct Machine {
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
        x = self.reflector[x as usize];
        x = self.left.inverse(x);
        x = self.mid.inverse(x);
        x = self.right.inverse(x);
        self.plug[x as usize]
    }
}

pub fn transform(
    input: &str,
    rotors: &str,
    reflector: &str,
    rings: &str,
    position: &str,
    plugboard: &str,
) -> Result<String> {
    let [mut left, mut mid, mut right] = parse_rotors(rotors)?;
    let ring = parse_three(rings, "rings")?;
    let pos = parse_three(position, "position")?;
    left.ring = ring[0];
    mid.ring = ring[1];
    right.ring = ring[2];
    left.pos = pos[0];
    mid.pos = pos[1];
    right.pos = pos[2];
    let mut machine = Machine {
        left,
        mid,
        right,
        reflector: named_reflector(reflector)?,
        plug: parse_plugboard(plugboard)?,
    };

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
