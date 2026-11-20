//! Packs: the look of the window. Each folder in app/packs/ is one pack: a
//! pack.txt of colours (see packs/blue-gold/pack.txt for every key) and the
//! pieces, baked by build.rs. T in the window switches packs.
use crate::canvas::{Color, Img};
use crate::game::live_dir;
use std::fs;

include!(concat!(env!("OUT_DIR"), "/packs.rs"));
pub const DEFAULT_PACK: &str = "blue-gold";

/// Every colour the window draws, by what it is for.
pub struct Theme {
    pub name: String,
    pub background: Color,
    pub accent: Color,
    pub title: Color,
    pub text: Color,
    pub muted: Color,
    pub error: Color,
    pub status_your_move: Color,
    pub status_waiting: Color,
    pub status_over: Color,
    pub board_border: Color,
    pub light_square: Color,
    pub dark_square: Color,
    pub last_from: Color,
    pub last_to: Color,
    pub last_outline: Color,
    pub selected: Color,
    pub target: Color,
    pub target_outline: Color,
    pub half_typed: Color,
    pub check: Color,
    pub check_outline: Color,
    pub shade: Color,
    pub banner: Color,
    pub banner_title: Color,
    pub banner_result: Color,
}

/// "#RRGGBB" or "#RRGGBBAA"
pub fn hex_color(v: &str) -> Option<Color> {
    let h = v.trim().strip_prefix('#')?;
    if !(h.len() == 6 || h.len() == 8) || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
    Some(Color::rgba8(
        n(0),
        n(2),
        n(4),
        if h.len() == 8 { n(6) } else { 255 },
    ))
}

/// key = value lines; a line starting with # is a comment; later lines win
fn pack_keys(text: &str, into: &mut std::collections::HashMap<String, String>) {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            into.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
}

pub fn pack_index(id: &str) -> Option<usize> {
    PACKS.iter().position(|p| p.0 == id)
}

impl Theme {
    /// A pack's colours laid over the default pack's, so a pack lists only what it changes.
    pub fn load(index: usize) -> Result<Theme, String> {
        let mut keys = std::collections::HashMap::new();
        if let Some(d) = pack_index(DEFAULT_PACK) {
            pack_keys(PACKS[d].1, &mut keys);
        }
        pack_keys(PACKS[index].1, &mut keys);
        let col = |k: &str| -> Result<Color, String> {
            let v = keys.get(k).ok_or(format!("{k} is missing"))?;
            hex_color(v).ok_or(format!("{k} = {v} is not a colour like #RRGGBB"))
        };
        Ok(Theme {
            name: keys
                .get("name")
                .cloned()
                .unwrap_or_else(|| PACKS[index].0.to_string()),
            background: col("background")?,
            accent: col("accent")?,
            title: col("title")?,
            text: col("text")?,
            muted: col("muted")?,
            error: col("error")?,
            status_your_move: col("status_your_move")?,
            status_waiting: col("status_waiting")?,
            status_over: col("status_over")?,
            board_border: col("board_border")?,
            light_square: col("light_square")?,
            dark_square: col("dark_square")?,
            last_from: col("last_from")?,
            last_to: col("last_to")?,
            last_outline: col("last_outline")?,
            selected: col("selected")?,
            target: col("target")?,
            target_outline: col("target_outline")?,
            half_typed: col("half_typed")?,
            check: col("check")?,
            check_outline: col("check_outline")?,
            shade: col("shade")?,
            banner: col("banner")?,
            banner_title: col("banner_title")?,
            banner_result: col("banner_result")?,
        })
    }
}

/// The pack picked last time (saved in ~/.side-chess/pack.txt), else the default.
pub fn saved_pack() -> usize {
    fs::read_to_string(live_dir().join("pack.txt"))
        .ok()
        .and_then(|id| pack_index(id.trim()))
        .or_else(|| pack_index(DEFAULT_PACK))
        .unwrap_or(0)
}

pub fn load_pack(index: usize) -> (Theme, Img) {
    let theme = Theme::load(index)
        .or_else(|_| Theme::load(pack_index(DEFAULT_PACK).unwrap_or(0)))
        .expect("the default pack loads");
    (theme, Img::from_png(PACKS[index].2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pack_loads_with_valid_colours_and_a_pieces_image() {
        assert!(
            pack_index(DEFAULT_PACK).is_some(),
            "the default pack must exist"
        );
        for (i, (id, _, png)) in PACKS.iter().enumerate() {
            if let Err(e) = Theme::load(i) {
                panic!("pack {id}: {e}");
            }
            // PNG signature, and 768 x 256 (six pieces across, white then black)
            assert_eq!(
                &png[..8],
                b"\x89PNG\r\n\x1a\n",
                "pack {id}: pieces is not a PNG"
            );
            let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
            assert_eq!(
                (w, h),
                (768, 256),
                "pack {id}: pieces.png must be 768 x 256"
            );
        }
    }

    #[test]
    fn colours_parse_with_and_without_alpha() {
        assert!(hex_color("#1152D4").is_some());
        assert_eq!(hex_color("#00000080").unwrap().a, 128.0 / 255.0);
        assert!(hex_color("1152D4").is_none());
        assert!(hex_color("#12345").is_none());
        assert!(hex_color("#GGGGGG").is_none());
    }
}
