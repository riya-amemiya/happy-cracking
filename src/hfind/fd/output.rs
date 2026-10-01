use std::collections::HashMap;
use std::fs::Metadata;
use std::path::Path;

use super::walk::Entry;
use super::{Config, Set};
use crate::hc_internal::walker::FileType;

pub(super) const DEFAULT_LS_COLORS: &str = "
ow=0:or=0;38;5;16;48;5;203:no=0:ex=1;38;5;203:cd=0;38;5;203;48;5;236:mi=0;38;5;16;48;5;203:*~=0;38;5;243:st=0:pi=0;38;5;16;48;5;81:fi=0:di=0;38;5;81:so=0;38;5;16;48;5;203:bd=0;38;5;81;48;5;236:tw=0:ln=0;38;5;203:*.m=0;38;5;48:*.o=0;38;5;243:*.z=4;38;5;203:*.a=1;38;5;203:*.r=0;38;5;48:*.c=0;38;5;48:*.d=0;38;5;48:*.t=0;38;5;48:*.h=0;38;5;48:*.p=0;38;5;48:*.cc=0;38;5;48:*.ll=0;38;5;48:*.jl=0;38;5;48:*css=0;38;5;48:*.md=0;38;5;185:*.gz=4;38;5;203:*.nb=0;38;5;48:*.mn=0;38;5;48:*.go=0;38;5;48:*.xz=4;38;5;203:*.so=1;38;5;203:*.rb=0;38;5;48:*.pm=0;38;5;48:*.bc=0;38;5;243:*.py=0;38;5;48:*.as=0;38;5;48:*.pl=0;38;5;48:*.rs=0;38;5;48:*.sh=0;38;5;48:*.7z=4;38;5;203:*.ps=0;38;5;186:*.cs=0;38;5;48:*.el=0;38;5;48:*.rm=0;38;5;208:*.hs=0;38;5;48:*.td=0;38;5;48:*.ui=0;38;5;149:*.ex=0;38;5;48:*.js=0;38;5;48:*.cp=0;38;5;48:*.cr=0;38;5;48:*.la=0;38;5;243:*.kt=0;38;5;48:*.ml=0;38;5;48:*.vb=0;38;5;48:*.gv=0;38;5;48:*.lo=0;38;5;243:*.hi=0;38;5;243:*.ts=0;38;5;48:*.ko=1;38;5;203:*.hh=0;38;5;48:*.pp=0;38;5;48:*.di=0;38;5;48:*.bz=4;38;5;203:*.fs=0;38;5;48:*.png=0;38;5;208:*.zsh=0;38;5;48:*.mpg=0;38;5;208:*.pid=0;38;5;243:*.xmp=0;38;5;149:*.iso=4;38;5;203:*.m4v=0;38;5;208:*.dot=0;38;5;48:*.ods=0;38;5;186:*.inc=0;38;5;48:*.sxw=0;38;5;186:*.aif=0;38;5;208:*.git=0;38;5;243:*.gvy=0;38;5;48:*.tbz=4;38;5;203:*.log=0;38;5;243:*.txt=0;38;5;185:*.ico=0;38;5;208:*.csx=0;38;5;48:*.vob=0;38;5;208:*.pgm=0;38;5;208:*.pps=0;38;5;186:*.ics=0;38;5;186:*.img=4;38;5;203:*.fon=0;38;5;208:*.hpp=0;38;5;48:*.bsh=0;38;5;48:*.sql=0;38;5;48:*TODO=1:*.php=0;38;5;48:*.pkg=4;38;5;203:*.ps1=0;38;5;48:*.csv=0;38;5;185:*.ilg=0;38;5;243:*.ini=0;38;5;149:*.pyc=0;38;5;243:*.psd=0;38;5;208:*.htc=0;38;5;48:*.swp=0;38;5;243:*.mli=0;38;5;48:*hgrc=0;38;5;149:*.bst=0;38;5;149:*.ipp=0;38;5;48:*.fsi=0;38;5;48:*.tcl=0;38;5;48:*.exs=0;38;5;48:*.out=0;38;5;243:*.jar=4;38;5;203:*.xls=0;38;5;186:*.ppm=0;38;5;208:*.apk=4;38;5;203:*.aux=0;38;5;243:*.rpm=4;38;5;203:*.dll=1;38;5;203:*.eps=0;38;5;208:*.exe=1;38;5;203:*.doc=0;38;5;186:*.wma=0;38;5;208:*.deb=4;38;5;203:*.pod=0;38;5;48:*.ind=0;38;5;243:*.nix=0;38;5;149:*.lua=0;38;5;48:*.epp=0;38;5;48:*.dpr=0;38;5;48:*.htm=0;38;5;185:*.ogg=0;38;5;208:*.bin=4;38;5;203:*.otf=0;38;5;208:*.yml=0;38;5;149:*.pro=0;38;5;149:*.cxx=0;38;5;48:*.tex=0;38;5;48:*.fnt=0;38;5;208:*.erl=0;38;5;48:*.sty=0;38;5;243:*.bag=4;38;5;203:*.rst=0;38;5;185:*.pdf=0;38;5;186:*.pbm=0;38;5;208:*.xcf=0;38;5;208:*.clj=0;38;5;48:*.gif=0;38;5;208:*.rar=4;38;5;203:*.elm=0;38;5;48:*.bib=0;38;5;149:*.tsx=0;38;5;48:*.dmg=4;38;5;203:*.tmp=0;38;5;243:*.bcf=0;38;5;243:*.mkv=0;38;5;208:*.svg=0;38;5;208:*.cpp=0;38;5;48:*.vim=0;38;5;48:*.bmp=0;38;5;208:*.ltx=0;38;5;48:*.fls=0;38;5;243:*.flv=0;38;5;208:*.wav=0;38;5;208:*.m4a=0;38;5;208:*.mid=0;38;5;208:*.hxx=0;38;5;48:*.pas=0;38;5;48:*.wmv=0;38;5;208:*.tif=0;38;5;208:*.kex=0;38;5;186:*.mp4=0;38;5;208:*.bak=0;38;5;243:*.xlr=0;38;5;186:*.dox=0;38;5;149:*.swf=0;38;5;208:*.tar=4;38;5;203:*.tgz=4;38;5;203:*.cfg=0;38;5;149:*.xml=0;
38;5;185:*.jpg=0;38;5;208:*.mir=0;38;5;48:*.sxi=0;38;5;186:*.bz2=4;38;5;203:*.odt=0;38;5;186:*.mov=0;38;5;208:*.toc=0;38;5;243:*.bat=1;38;5;203:*.asa=0;38;5;48:*.awk=0;38;5;48:*.sbt=0;38;5;48:*.vcd=4;38;5;203:*.kts=0;38;5;48:*.arj=4;38;5;203:*.blg=0;38;5;243:*.c++=0;38;5;48:*.odp=0;38;5;186:*.bbl=0;38;5;243:*.idx=0;38;5;243:*.com=1;38;5;203:*.mp3=0;38;5;208:*.avi=0;38;5;208:*.def=0;38;5;48:*.cgi=0;38;5;48:*.zip=4;38;5;203:*.ttf=0;38;5;208:*.ppt=0;38;5;186:*.tml=0;38;5;149:*.fsx=0;38;5;48:*.h++=0;38;5;48:*.rtf=0;38;5;186:*.inl=0;38;5;48:*.yaml=0;38;5;149:*.html=0;38;5;185:*.mpeg=0;38;5;208:*.java=0;38;5;48:*.hgrc=0;38;5;149:*.orig=0;38;5;243:*.conf=0;38;5;149:*.dart=0;38;5;48:*.psm1=0;38;5;48:*.rlib=0;38;5;243:*.fish=0;38;5;48:*.bash=0;38;5;48:*.make=0;38;5;149:*.docx=0;38;5;186:*.json=0;38;5;149:*.psd1=0;38;5;48:*.lisp=0;38;5;48:*.tbz2=4;38;5;203:*.diff=0;38;5;48:*.epub=0;38;5;186:*.xlsx=0;38;5;186:*.pptx=0;38;5;186:*.toml=0;38;5;149:*.h264=0;38;5;208:*.purs=0;38;5;48:*.flac=0;38;5;208:*.tiff=0;38;5;208:*.jpeg=0;38;5;208:*.lock=0;38;5;243:*.less=0;38;5;48:*.dyn_o=0;38;5;243:*.scala=0;38;5;48:*.mdown=0;38;5;185:*.shtml=0;38;5;185:*.class=0;38;5;243:*.cache=0;38;5;243:*.cmake=0;38;5;149:*passwd=0;38;5;149:*.swift=0;38;5;48:*shadow=0;38;5;149:*.xhtml=0;38;5;185:*.patch=0;38;5;48:*.cabal=0;38;5;48:*README=0;38;5;16;48;5;186:*.toast=4;38;5;203:*.ipynb=0;38;5;48:*COPYING=0;38;5;249:*.gradle=0;38;5;48:*.matlab=0;38;5;48:*.config=0;38;5;149:*LICENSE=0;38;5;249:*.dyn_hi=0;38;5;243:*.flake8=0;38;5;149:*.groovy=0;38;5;48:*INSTALL=0;38;5;16;48;5;186:*TODO.md=1:*.ignore=0;38;5;149:*Doxyfile=0;38;5;149:*TODO.txt=1:*setup.py=0;38;5;149:*Makefile=0;38;5;149:*.gemspec=0;38;5;149:*.desktop=0;38;5;149:*.rgignore=0;38;5;149:*.markdown=0;38;5;185:*COPYRIGHT=0;38;5;249:*configure=0;38;5;149:*.DS_Store=0;38;5;243:*.kdevelop=0;38;5;149:*.fdignore=0;38;5;149:*README.md=0;38;5;16;48;5;186:*.cmake.in=0;38;5;149:*SConscript=0;38;5;149:*CODEOWNERS=0;38;5;149:*.localized=0;38;5;243:*.gitignore=0;38;5;149:*Dockerfile=0;38;5;149:*.gitconfig=0;38;5;149:*INSTALL.md=0;38;5;16;48;5;186:*README.txt=0;38;5;16;48;5;186:*SConstruct=0;38;5;149:*.scons_opt=0;38;5;243:*.travis.yml=0;38;5;186:*.gitmodules=0;38;5;149:*.synctex.gz=0;38;5;243:*LICENSE-MIT=0;38;5;249:*MANIFEST.in=0;38;5;149:*Makefile.in=0;38;5;243:*Makefile.am=0;38;5;149:*INSTALL.txt=0;38;5;16;48;5;186:*configure.ac=0;38;5;149:*.applescript=0;38;5;48:*appveyor.yml=0;38;5;186:*.fdb_latexmk=0;38;5;243:*CONTRIBUTORS=0;38;5;16;48;5;186:*.clang-format=0;38;5;149:*LICENSE-APACHE=0;38;5;249:*CMakeLists.txt=0;38;5;149:*CMakeCache.txt=0;38;5;243:*.gitattributes=0;38;5;149:*CONTRIBUTORS.md=0;38;5;16;48;5;186:*.sconsign.dblite=0;38;5;243:*requirements.txt=0;38;5;149:*CONTRIBUTORS.txt=0;38;5;16;48;5;186:*package-lock.json=0;38;5;243:*.CFUserTextEncoding=0;38;5;243
";

const LS_COLORS_DEFAULT: &str = "rs=0:lc=\x1b[:rc=m:cl=\x1b[K:ex=01;32:sg=30;43:su=37;41:di=01;34:st=37;44:ow=34;42:tw=30;42:ln=01;36:bd=01;33:cd=01;33:do=01;35:pi=33:so=01;35:";

const INDICATORS: [&str; 24] = [
    "no", "fi", "di", "ln", "pi", "so", "do", "bd", "cd", "or", "su", "sg", "st", "ow", "tw", "ex",
    "mi", "ca", "mh", "lc", "rc", "ec", "rs", "cl",
];

const NORMAL: usize = 0;
const FILE: usize = 1;
const DIR: usize = 2;
const LINK: usize = 3;
const FIFO: usize = 4;
const SOCKET: usize = 5;
const BLOCK: usize = 7;
const CHAR: usize = 8;
const ORPHAN: usize = 9;
const SETUID: usize = 10;
const SETGID: usize = 11;
const STICKY: usize = 12;
const OTHER_WRITABLE: usize = 13;
const STICKY_OTHER_WRITABLE: usize = 14;
const EXEC: usize = 15;
const MISSING: usize = 16;
const MULTI_LINK: usize = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Color {
    Basic(u8),
    Fixed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(super) struct Style {
    fg: Option<Color>,
    bg: Option<Color>,
    font: [bool; 9],
    underline: Option<Color>,
}

fn extended_color(parts: &mut std::collections::VecDeque<u8>) -> Option<Color> {
    match (parts.pop_front(), parts.pop_front()) {
        (Some(5), Some(n)) => Some(Color::Fixed(n)),
        (Some(2), Some(r)) => match (parts.pop_front(), parts.pop_front()) {
            (Some(g), Some(b)) => Some(Color::Rgb(r, g, b)),
            _ => None,
        },
        _ => None,
    }
}

impl Style {
    fn from_ansi_sequence(code: &str) -> Option<Style> {
        if code.is_empty() || code == "0" || code == "00" {
            return None;
        }
        let mut parts: std::collections::VecDeque<u8> = code
            .split(';')
            .map(|c| c.parse::<u8>().ok())
            .collect::<Option<_>>()?;
        let mut s = Style::default();
        while let Some(p) = parts.pop_front() {
            match p {
                0 => s.font = [false; 9],
                1..=9 => s.font[usize::from(p) - 1] = true,
                22 => {
                    s.font[0] = false;
                    s.font[1] = false;
                }
                23 => s.font[2] = false,
                24 => s.font[3] = false,
                25 => {
                    s.font[4] = false;
                    s.font[5] = false;
                }
                27 => s.font[6] = false,
                28 => s.font[7] = false,
                29 => s.font[8] = false,
                30..=37 => s.fg = Some(Color::Basic(p - 30)),
                38 => match extended_color(&mut parts) {
                    Some(c) => s.fg = Some(c),
                    None => break,
                },
                39 => s.fg = None,
                40..=47 => s.bg = Some(Color::Basic(p - 40)),
                48 => match extended_color(&mut parts) {
                    Some(c) => s.bg = Some(c),
                    None => break,
                },
                49 => s.bg = None,
                58 => match extended_color(&mut parts) {
                    Some(c) => s.underline = Some(c),
                    None => break,
                },
                59 => s.underline = None,
                90..=97 => s.fg = Some(Color::Fixed(p - 90 + 8)),
                100..=107 => s.bg = Some(Color::Fixed(p - 100 + 8)),
                _ => {}
            }
        }
        Some(s)
    }

    fn prefix(&self) -> String {
        const CODES: [&str; 8] = ["1", "2", "3", "4", "5", "7", "8", "9"];
        let font = [
            self.font[0],
            self.font[1],
            self.font[2],
            self.font[3],
            self.font[4] || self.font[5],
            self.font[6],
            self.font[7],
            self.font[8],
        ];
        let mut parts: Vec<String> = font
            .iter()
            .zip(CODES)
            .filter(|(on, _)| **on)
            .map(|(_, code)| code.to_string())
            .collect();
        let code = |c: Color, base: u8| match c {
            Color::Basic(n) => (base + n).to_string(),
            Color::Fixed(n) => format!("{};5;{n}", base + 8),
            Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
        };
        if let Some(bg) = self.bg {
            parts.push(code(bg, 40));
        }
        if let Some(fg) = self.fg {
            parts.push(code(fg, 30));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("\x1b[{}m", parts.join(";"))
        }
    }
}

pub(super) struct Paint {
    prefix: String,
}

impl Paint {
    fn from(style: Option<&Style>) -> Paint {
        Paint {
            prefix: style.map_or_else(String::new, Style::prefix),
        }
    }

    pub(super) fn paint(&self, out: &mut Vec<u8>, text: &[u8]) {
        if self.prefix.is_empty() {
            out.extend_from_slice(text);
        } else {
            out.extend_from_slice(self.prefix.as_bytes());
            out.extend_from_slice(text);
            out.extend_from_slice(b"\x1b[0m");
        }
    }
}

struct Suffix {
    index: usize,
    original: Vec<u8>,
    insensitive: bool,
}

pub(super) struct LsColors {
    indicators: [Option<Style>; 24],
    file_normal_fallback: bool,
    by_lower: HashMap<Vec<u8>, Vec<Suffix>>,
    lengths: Vec<usize>,
    styles: Vec<Option<Style>>,
    paints: Vec<Paint>,
    indicator_paints: Vec<Paint>,
    plain: Paint,
}

impl LsColors {
    pub(super) fn from_env_or_default() -> LsColors {
        match std::env::var("LS_COLORS") {
            Ok(s) => Self::from_string(&s),
            Err(_) => Self::from_string(DEFAULT_LS_COLORS),
        }
    }

    pub(super) fn from_string(input: &str) -> LsColors {
        let mut indicators: [Option<Style>; 24] = Default::default();
        let mut file_normal_fallback = true;
        let mut keys: Vec<(Vec<u8>, Option<Style>)> = Vec::new();
        for text in [LS_COLORS_DEFAULT, input] {
            for entry in text.split(':') {
                let parts: Vec<&str> = entry.split('=').collect();
                let [key, ansi, ..] = parts[..] else { continue };
                let style = Style::from_ansi_sequence(ansi);
                if let Some(suffix) = key.strip_prefix('*') {
                    keys.push((suffix.as_bytes().to_vec(), style));
                } else if let Some(i) = INDICATORS.iter().position(|&n| n == key) {
                    if style.is_none() && i == FILE {
                        file_normal_fallback = false;
                    }
                    indicators[i] = style;
                }
            }
        }
        keys.reverse();
        let lower: Vec<Vec<u8>> = keys.iter().map(|(k, _)| k.to_ascii_lowercase()).collect();
        let mut by_case: HashMap<&[u8], usize> = HashMap::new();
        let mut by_fold: HashMap<&[u8], usize> = HashMap::new();
        for (i, (k, _)) in keys.iter().enumerate() {
            by_case.entry(k).or_insert(i);
            by_fold.entry(&lower[i]).or_insert(i);
        }
        let cs_only: std::collections::HashSet<&[u8]> = by_case
            .values()
            .filter(|&&i| keys[i].1 != keys[by_fold[lower[i].as_slice()]].1)
            .map(|&i| lower[i].as_slice())
            .collect();
        let mut by_lower: HashMap<Vec<u8>, Vec<Suffix>> = HashMap::new();
        let mut lengths: Vec<usize> = Vec::new();
        for (i, (k, _)) in keys.iter().enumerate() {
            if !lengths.contains(&k.len()) {
                lengths.push(k.len());
            }
            by_lower.entry(lower[i].clone()).or_default().push(Suffix {
                index: i,
                original: k.clone(),
                insensitive: !cs_only.contains(lower[i].as_slice()),
            });
        }
        let styles: Vec<Option<Style>> = keys.into_iter().map(|(_, s)| s).collect();
        let paints = styles.iter().map(|s| Paint::from(s.as_ref())).collect();
        let mut colors = LsColors {
            indicators,
            file_normal_fallback,
            by_lower,
            lengths,
            styles,
            paints,
            indicator_paints: Vec::new(),
            plain: Paint::from(None),
        };
        colors.indicator_paints = (0..24)
            .map(|i| Paint::from(colors.style_for_indicator(i)))
            .collect();
        colors
    }

    fn has(&self, i: usize) -> bool {
        self.indicators[i].is_some()
    }

    fn suffix_index(&self, name: &[u8]) -> Option<usize> {
        self.lengths
            .iter()
            .filter(|&&len| len <= name.len())
            .filter_map(|&len| {
                let tail = &name[name.len() - len..];
                self.by_lower
                    .get(&tail.to_ascii_lowercase())?
                    .iter()
                    .filter(|s| s.insensitive || s.original == tail)
                    .map(|s| s.index)
                    .min()
            })
            .min()
    }

    fn style_for_indicator(&self, i: usize) -> Option<&Style> {
        self.indicators[i]
            .as_ref()
            .or_else(|| {
                let fallback = match i {
                    SETUID | SETGID | EXEC | MULTI_LINK => FILE,
                    STICKY_OTHER_WRITABLE | OTHER_WRITABLE | STICKY => DIR,
                    ORPHAN => LINK,
                    MISSING => ORPHAN,
                    _ => i,
                };
                self.indicators[fallback].as_ref()
            })
            .or_else(|| {
                if i == FILE && !self.file_normal_fallback {
                    None
                } else {
                    self.indicators[NORMAL].as_ref()
                }
            })
    }

    pub(super) fn directory(&self) -> &Paint {
        &self.indicator_paints[DIR]
    }

    fn indicator_for(
        &self,
        ft: Option<FileType>,
        meta: impl Fn() -> Option<Metadata>,
        path: &Path,
    ) -> usize {
        let Some(ft) = ft else { return FILE };
        if ft.is_file() {
            if (self.has(SETUID) || self.has(SETGID) || self.has(EXEC) || self.has(MULTI_LINK))
                && let Some(md) = meta()
            {
                let (mode, nlink) = mode_nlink(&md);
                if self.has(SETUID) && mode & 0o4000 != 0 {
                    return SETUID;
                } else if self.has(SETGID) && mode & 0o2000 != 0 {
                    return SETGID;
                } else if self.has(EXEC) && mode & 0o111 != 0 {
                    return EXEC;
                } else if self.has(MULTI_LINK) && nlink > 1 {
                    return MULTI_LINK;
                }
            }
            FILE
        } else if ft.is_dir() {
            if (self.has(STICKY_OTHER_WRITABLE) || self.has(OTHER_WRITABLE) || self.has(STICKY))
                && let Some(md) = meta()
            {
                let (mode, _) = mode_nlink(&md);
                if self.has(STICKY_OTHER_WRITABLE) && mode & 0o1002 == 0o1002 {
                    return STICKY_OTHER_WRITABLE;
                } else if self.has(OTHER_WRITABLE) && mode & 0o002 != 0 {
                    return OTHER_WRITABLE;
                } else if self.has(STICKY) && mode & 0o1000 != 0 {
                    return STICKY;
                }
            }
            DIR
        } else if ft.is_symlink() {
            if self.has(ORPHAN) && !path.exists() {
                return ORPHAN;
            }
            LINK
        } else if ft.is_fifo() {
            FIFO
        } else if ft.is_socket() {
            SOCKET
        } else if ft.is_block_device() {
            BLOCK
        } else if ft.is_char_device() {
            CHAR
        } else {
            MISSING
        }
    }

    pub(super) fn paint_for(&self, entry: &Entry) -> &Paint {
        let indicator = self.indicator_for(
            entry.file_type(),
            || entry.metadata().cloned(),
            entry.path(),
        );
        if indicator == FILE {
            let Some(name) = entry.color_name().to_str() else {
                return &self.plain;
            };
            if let Some(i) = self.suffix_index(name.as_bytes())
                && self.styles[i].is_some()
            {
                return &self.paints[i];
            }
        }
        &self.indicator_paints[indicator]
    }
}

#[cfg(unix)]
fn mode_nlink(md: &Metadata) -> (u32, u64) {
    use std::os::unix::fs::MetadataExt;
    (md.mode(), md.nlink())
}

#[cfg(not(unix))]
fn mode_nlink(_: &Metadata) -> (u32, u64) {
    (0, 1)
}

#[cfg(unix)]
fn host() -> &'static str {
    use std::sync::OnceLock;
    static HOSTNAME: OnceLock<String> = OnceLock::new();
    HOSTNAME.get_or_init(|| {
        unsafe extern "C" {
            fn gethostname(name: *mut std::ffi::c_char, len: usize) -> std::ffi::c_int;
        }
        let mut buf = vec![0u8; 256];
        let ok = unsafe { gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
        if !ok {
            return String::new();
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        buf.truncate(end);
        String::from_utf8(buf).unwrap_or_default()
    })
}

#[cfg(not(unix))]
fn host() -> &'static str {
    "/"
}

pub(super) fn hyperlink_start(out: &mut Vec<u8>, abs: &[u8]) {
    out.extend_from_slice(b"\x1B]8;;file://");
    out.extend_from_slice(host().as_bytes());
    for &byte in abs {
        match byte {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'/' | b':' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte);
            }
            b'\\' if cfg!(windows) => out.push(b'/'),
            _ => {
                out.extend_from_slice(format!("%{byte:02X}").as_bytes());
            }
        }
    }
    out.extend_from_slice(b"\x1B\\");
}

fn replace_sep(text: &str, sep: &str) -> String {
    text.replace(std::path::MAIN_SEPARATOR, sep)
}

pub(super) fn print_entry(out: &mut Vec<u8>, entry: &Entry, config: &Config) {
    let hyperlink = config.is(Set::Hyperlink)
        && super::fsx::path_absolute_form(entry.path())
            .map(|abs| hyperlink_start(out, abs.as_os_str().as_encoded_bytes()))
            .is_ok();
    let stripped = entry.stripped_path(config);
    if let Some(format) = &config.format {
        let text = format.generate(stripped, config.path_separator.as_deref());
        out.extend_from_slice(text.to_string_lossy().as_bytes());
    } else if let Some(colors) = &config.ls_colors {
        print_colorized(out, entry, stripped, config, colors);
    } else if cfg!(unix) && !config.is(Set::InteractiveTerminal) && config.path_separator.is_none()
    {
        out.extend_from_slice(stripped.as_os_str().as_encoded_bytes());
        trailing_slash(out, entry, config, None);
    } else {
        let text = stripped.to_string_lossy();
        match &config.path_separator {
            Some(sep) => out.extend_from_slice(replace_sep(&text, sep).as_bytes()),
            None => out.extend_from_slice(text.as_bytes()),
        }
        trailing_slash(out, entry, config, None);
    }
    if hyperlink {
        out.extend_from_slice(b"\x1B]8;;\x1B\\");
    }
    out.push(if config.is(Set::NullSeparator) {
        0
    } else {
        b'\n'
    });
}

fn trailing_slash(out: &mut Vec<u8>, entry: &Entry, config: &Config, paint: Option<&Paint>) {
    if matches!(entry.file_type(), Some(ft) if ft.is_dir()) {
        let sep = config.actual_path_separator.as_bytes();
        match paint {
            Some(p) => p.paint(out, sep),
            None => out.extend_from_slice(sep),
        }
    }
}

fn print_colorized(
    out: &mut Vec<u8>,
    entry: &Entry,
    path: &Path,
    config: &Config,
    colors: &LsColors,
) {
    let path_str = path.to_string_lossy();
    let offset = path.parent().map_or(0, |parent| {
        let start = parent.to_string_lossy().len();
        start
            + path_str[start..]
                .chars()
                .take_while(|&c| std::path::is_separator(c))
                .map(char::len_utf8)
                .sum::<usize>()
    });
    if offset > 0 {
        let parent = &path_str[..offset];
        match &config.path_separator {
            Some(sep) => colors
                .directory()
                .paint(out, replace_sep(parent, sep).as_bytes()),
            None => colors.directory().paint(out, parent.as_bytes()),
        }
    }
    colors
        .paint_for(entry)
        .paint(out, path_str[offset..].as_bytes());
    trailing_slash(out, entry, config, Some(colors.directory()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(colors: &LsColors, name: &str) -> String {
        colors
            .suffix_index(name.as_bytes())
            .and_then(|i| colors.styles[i].as_ref())
            .map_or_else(String::new, Style::prefix)
    }

    #[test]
    fn suffix_rules_follow_lscolors() {
        let c = LsColors::from_string("*.jpg=01;35:*.Z=01;31");
        assert_eq!(prefix(&c, "img1.JpG"), "\x1b[1;35m");
        let c = LsColors::from_string("*.jpg=01;35:*.JPG=01;32");
        assert_eq!(prefix(&c, "img1.jpg"), "\x1b[1;35m");
        assert_eq!(prefix(&c, "img1.JPG"), "\x1b[1;32m");
        assert_eq!(prefix(&c, "img1.JpG"), "");
        let c = LsColors::from_string("*.jpg=01;32:*.jpg=01;35:*.JPG=01;32");
        assert_eq!(prefix(&c, "img1.jpg"), "\x1b[1;35m");
        assert_eq!(prefix(&c, "img1.JPG"), "\x1b[1;32m");
        assert_eq!(prefix(&c, "img1.JpG"), "");
        let c = LsColors::from_string("*.foo=01;35:*README.foo=33;44");
        assert_eq!(prefix(&c, "README.foo"), "\x1b[44;33m");
        let c = LsColors::from_string("*README.foo=33;44:*.foo=01;35");
        assert_eq!(prefix(&c, "README.foo"), "\x1b[1;35m");
        let c = LsColors::from_string("*.png=01;35:*.png=0");
        assert!(
            c.suffix_index(b"a.png")
                .is_some_and(|i| c.styles[i].is_none())
        );
    }

    #[test]
    fn styles_and_indicators() {
        let c = LsColors::from_string(DEFAULT_LS_COLORS);
        assert_eq!(c.directory().prefix, "\x1b[38;5;81m");
        assert!(c.has(OTHER_WRITABLE));
        assert!(!c.has(STICKY));
        assert_eq!(
            Style::from_ansi_sequence("38;2;255;0;100;1;4")
                .unwrap()
                .prefix(),
            "\x1b[1;4;38;2;255;0;100m"
        );
        assert_eq!(
            Style::from_ansi_sequence("91;101").unwrap().prefix(),
            "\x1b[48;5;9;38;5;9m"
        );
        assert!(Style::from_ansi_sequence("0").is_none());
        assert!(Style::from_ansi_sequence("x").is_none());
        let c = LsColors::from_string("no=01;31:fi=0");
        assert!(c.style_for_indicator(FILE).is_none());
        let c = LsColors::from_string("mi=01:or=33;44");
        assert_eq!(c.style_for_indicator(MISSING).unwrap().prefix(), "\x1b[1m");
        let mut out = Vec::new();
        Paint::from(None).paint(&mut out, b"x");
        assert_eq!(out, b"x");
        let mut out = Vec::new();
        hyperlink_start(&mut out, b"/a b");
        assert!(out.ends_with(b"/a%20b\x1B\\"));
    }
}
