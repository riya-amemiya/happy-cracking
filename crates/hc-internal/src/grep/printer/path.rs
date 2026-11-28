use std::io;
use std::path::Path;

use super::color::ColorSpecs;
use super::hyperlink::{self, HyperlinkConfig};
use super::termcolor::WriteColor;
use super::util::PrinterPath;

#[derive(Clone, Debug)]
struct Config {
    colors: ColorSpecs,
    hyperlink: HyperlinkConfig,
    separator: Option<u8>,
    terminator: u8,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            colors: ColorSpecs::default(),
            hyperlink: HyperlinkConfig::default(),
            separator: None,
            terminator: b'\n',
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PathPrinterBuilder {
    config: Config,
}

impl PathPrinterBuilder {
    #[must_use]
    pub fn new() -> PathPrinterBuilder {
        PathPrinterBuilder::default()
    }

    pub fn build<W: WriteColor>(&self, wtr: W) -> PathPrinter<W> {
        let interpolator = hyperlink::Interpolator::new(&self.config.hyperlink);
        PathPrinter {
            config: self.config.clone(),
            wtr,
            interpolator,
        }
    }

    pub fn color_specs(&mut self, specs: ColorSpecs) -> &mut PathPrinterBuilder {
        self.config.colors = specs;
        self
    }

    pub fn hyperlink(&mut self, config: HyperlinkConfig) -> &mut PathPrinterBuilder {
        self.config.hyperlink = config;
        self
    }

    pub fn separator(&mut self, sep: Option<u8>) -> &mut PathPrinterBuilder {
        self.config.separator = sep;
        self
    }

    pub fn terminator(&mut self, terminator: u8) -> &mut PathPrinterBuilder {
        self.config.terminator = terminator;
        self
    }
}

#[derive(Debug)]
pub struct PathPrinter<W> {
    config: Config,
    wtr: W,
    interpolator: hyperlink::Interpolator,
}

impl<W: WriteColor> PathPrinter<W> {
    pub fn write(&mut self, path: &Path) -> io::Result<()> {
        let ppath = PrinterPath::new(path).with_separator(self.config.separator);
        if self.wtr.supports_color() {
            let enabled = self.interpolator.enabled(&self.wtr);
            let status = match enabled.then(|| ppath.as_hyperlink()).flatten() {
                Some(hyperpath) => {
                    let values = hyperlink::Values::new(hyperpath);
                    self.interpolator.begin(&values, &mut self.wtr)?
                }
                None => hyperlink::InterpolatorStatus::inactive(),
            };
            self.wtr.set_color(self.config.colors.path())?;
            self.wtr.write_all(ppath.as_bytes())?;
            self.wtr.reset()?;
            status.finish(&mut self.wtr)?;
        } else {
            self.wtr.write_all(ppath.as_bytes())?;
        }
        self.wtr.write_all(&[self.config.terminator])
    }

    pub fn get_mut(&mut self) -> &mut W {
        &mut self.wtr
    }
}
