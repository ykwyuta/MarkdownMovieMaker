//! 読みの辞書（`data/voice_dictionary.csv`：`表記,ひらがなの読み`）。
//!
//! Kokoro では合成前に表記を読みへ置き換える。VOICEVOX ではエンジンのユーザー辞書へ登録する。
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct Dictionary {
    /// 長い表記から順に並べる（部分一致の取り違えを防ぐ）。
    pub entries: Vec<(String, String)>,
}

impl Dictionary {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(Self::parse(&std::fs::read_to_string(path)?))
    }

    pub fn parse(text: &str) -> Self {
        let mut entries: Vec<(String, String)> = text
            .lines()
            .filter_map(|l| {
                let (a, b) = l.split_once(',')?;
                let unquote = |s: &str| s.trim().trim_matches('"').to_owned();
                let (a, b) = (unquote(a), unquote(b));
                (!a.is_empty() && !b.is_empty()).then_some((a, b))
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.0.chars().count()));
        Self { entries }
    }

    pub fn apply(&self, text: &str) -> String {
        if self.entries.is_empty() {
            return text.to_owned();
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        'outer: while !rest.is_empty() {
            for (surface, reading) in &self.entries {
                if let Some(r) = rest.strip_prefix(surface.as_str()) {
                    out.push_str(reading);
                    rest = r;
                    continue 'outer;
                }
            }
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
        out
    }
}

/// ひらがな → カタカナ（VOICEVOX のユーザー辞書は発音をカタカナで受け取る）。
#[cfg_attr(not(feature = "voicevox"), allow(dead_code))]
pub fn hira_to_kata(s: &str) -> String {
    s.chars()
        .map(|c| {
            if ('ぁ'..='ゖ').contains(&c) {
                char::from_u32(c as u32 + 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_longest_first() {
        let d = Dictionary::parse("o1,\"おーわん\"\nR1,\"あーるわん\"\nR,あーる\n");
        assert_eq!(d.apply("R1とo1とR"), "あーるわんとおーわんとあーる");
        assert_eq!(hira_to_kata("おーわん"), "オーワン");
    }
}
