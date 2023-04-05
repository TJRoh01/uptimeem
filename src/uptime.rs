use crate::color::Color;

#[derive(Debug)]
pub enum Uptime {
    UpUnknown,
    UpMax,
    Up99_99,
    Up99_95,
    Up99_9,
    Up99_8,
    Up99_5,
    Up99,
    Up98,
    Up97,
    Up95,
    Up90,
    UpMin
}

impl Uptime {
    pub fn as_str(&self) -> (&'static str, &'static str) {
        match self {
            Self::UpUnknown => ("??%", Color::LightGrey.as_str()),
            Self::UpMax => (">99.99%", Color::BrightGreen.as_str()),
            Self::Up99_99 => ("99.99%", Color::BrightGreen.as_str()),
            Self::Up99_95 => ("99.95%", Color::Green.as_str()),
            Self::Up99_9 => ("99.9%", Color::Green.as_str()),
            Self::Up99_8 => ("99.8%", Color::YellowGreen.as_str()),
            Self::Up99_5 => ("99.5%", Color::YellowGreen.as_str()),
            Self::Up99 => ("99%", Color::Yellow.as_str()),
            Self::Up98 => ("98%", Color::Yellow.as_str()),
            Self::Up97 => ("97%", Color::Orange.as_str()),
            Self::Up95 => ("95%", Color::Orange.as_str()),
            Self::Up90 => ("90%", Color::Red.as_str()),
            Self::UpMin => ("<90%", Color::Red.as_str())
        }
    }

    pub fn from_f64(x: f64) -> Self {
        match x {
            x if x >= 0.99995 => Self::UpMax,
            x if x >= 0.9999 => Self::Up99_99,
            x if x >= 0.9995 => Self::Up99_95,
            x if x >= 0.999 => Self::Up99_9,
            x if x >= 0.998 => Self::Up99_8,
            x if x >= 0.995 => Self::Up99_5,
            x if x >= 0.99 => Self::Up99,
            x if x >= 0.98 => Self::Up98,
            x if x >= 0.97 => Self::Up97,
            x if x >= 0.95 => Self::Up95,
            x if x >= 0.90 => Self::Up90,
            _ => Self::UpMin
        }
    }
}