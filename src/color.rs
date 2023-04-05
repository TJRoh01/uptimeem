//! # Named colors in shields.io
//! - brightgreen
//! - green
//! - yellowgreen
//! - yellow
//! - orange
//! - red
//! - blue
//! - blueviolet
//! - lightgrey
//! - success
//! - important
//! - critical
//! - informational
//! - inactive

#[derive(Debug)]
pub enum Color {
    //Custom(&'static str),
    BrightGreen,
    Green,
    YellowGreen,
    Yellow,
    Orange,
    Red,
    LightGrey
}

impl Color {
    pub fn as_str(&self) -> &'static str {
        match self {
            //Self::Custom(x) => x,
            Self::BrightGreen => "brightgreen",
            Self::Green => "green",
            Self::YellowGreen => "yellowgreen",
            Self::Yellow => "yellow",
            Self::Orange => "orange",
            Self::Red => "red",
            Self::LightGrey => "lightgrey"
        }
    }
}