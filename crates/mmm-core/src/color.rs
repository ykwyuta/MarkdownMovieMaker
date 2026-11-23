//! 色指定。Pillow と同じ色名、`#rrggbb[aa]`、`[r, g, b(, a)]` を受け付ける。
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    pub const WHITE: Self = Self([255, 255, 255, 255]);

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            let v = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
            let short = |i: usize| {
                u8::from_str_radix(hex.get(i..i + 1)?, 16)
                    .ok()
                    .map(|v| v * 17)
            };
            return match hex.len() {
                3 => Some(Self([short(0)?, short(1)?, short(2)?, 255])),
                4 => Some(Self([short(0)?, short(1)?, short(2)?, short(3)?])),
                6 => Some(Self([v(0)?, v(2)?, v(4)?, 255])),
                8 => Some(Self([v(0)?, v(2)?, v(4)?, v(6)?])),
                _ => None,
            };
        }
        let rgb = match s.to_ascii_lowercase().as_str() {
            "black" => [0, 0, 0],
            "white" => [255, 255, 255],
            "red" => [255, 0, 0],
            "green" => [0, 128, 0],
            "lime" => [0, 255, 0],
            "blue" => [0, 0, 255],
            "yellow" => [255, 255, 0],
            "orange" => [255, 165, 0],
            "purple" => [128, 0, 128],
            "pink" => [255, 192, 203],
            "gray" | "grey" => [128, 128, 128],
            "brown" => [165, 42, 42],
            "cyan" | "aqua" => [0, 255, 255],
            "magenta" | "fuchsia" => [255, 0, 255],
            "navy" => [0, 0, 128],
            "darkgreen" => [0, 100, 0],
            "skyblue" => [135, 206, 235],
            "transparent" => return Some(Self([0, 0, 0, 0])),
            _ => return None,
        };
        Some(Self([rgb[0], rgb[1], rgb[2], 255]))
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Name(String),
            List(Vec<u8>),
        }
        match Raw::deserialize(d)? {
            Raw::Name(s) => {
                Self::parse(&s).ok_or_else(|| de::Error::custom(format!("未知の色: {s}")))
            }
            Raw::List(v) => match v.as_slice() {
                [r, g, b] => Ok(Self([*r, *g, *b, 255])),
                [r, g, b, a] => Ok(Self([*r, *g, *b, *a])),
                _ => Err(de::Error::custom(
                    "色は [r, g, b] か [r, g, b, a] で指定してください",
                )),
            },
        }
    }
}

impl Serialize for Rgba {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let [r, g, b, a] = self.0;
        s.serialize_str(&format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_forms() {
        assert_eq!(Rgba::parse("green"), Some(Rgba([0, 128, 0, 255])));
        assert_eq!(Rgba::parse("#ff000080"), Some(Rgba([255, 0, 0, 128])));
        let c: Rgba = serde_json::from_str("[255,255,255,200]").unwrap();
        assert_eq!(c, Rgba([255, 255, 255, 200]));
        assert!(serde_json::from_str::<Rgba>("\"nope\"").is_err());
    }
}
