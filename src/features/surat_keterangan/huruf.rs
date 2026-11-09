//! Metrics and encoding for the standard PDF fonts Times-Roman and Times-Bold, which every PDF
//! reader has built in, so no font file needs to be embedded.
//!
//! The widths come from Adobe's AFM files for the core 14 fonts, which may be used and
//! distributed for any purpose. They are listed per WinAnsiEncoding code from 32 to 255, in
//! thousandths of the font size. 0 marks a code with no glyph.

#[derive(Clone, Copy)]
pub enum Huruf {
    Biasa,
    Tebal,
}

impl Huruf {
    fn lebar_kode(self, kode: u8) -> u16 {
        let tabel = match self {
            Huruf::Biasa => &TIMES_ROMAN,
            Huruf::Tebal => &TIMES_BOLD,
        };
        kode.checked_sub(32).map_or(0, |i| tabel[usize::from(i)])
    }

    /// Width of already encoded text, in points.
    pub fn lebar(self, teks: &[u8], ukuran: f32) -> f32 {
        let total: u32 = teks
            .iter()
            .map(|&kode| u32::from(self.lebar_kode(kode)))
            .sum();
        total as f32 * ukuran / 1000.0
    }

    /// Encodes for WinAnsiEncoding. A character the font cannot show becomes "?", so the text
    /// stays readable instead of silently losing letters.
    pub fn encode(self, teks: &str) -> Vec<u8> {
        teks.chars()
            .map(|c| match win_ansi(c) {
                Some(kode) if self.lebar_kode(kode) > 0 => kode,
                _ => b'?',
            })
            .collect()
    }

    /// Splits text into lines no wider than `maks`, breaking at plain spaces only, so a
    /// non-breaking space keeps its neighbours together. A single word wider than that keeps its
    /// own line rather than being cut.
    pub fn pecah_baris(self, teks: &str, ukuran: f32, maks: f32) -> Vec<Vec<u8>> {
        let spasi = self.lebar(b" ", ukuran);
        let mut baris: Vec<Vec<u8>> = Vec::new();
        let mut sekarang: Vec<u8> = Vec::new();
        let mut lebar_sekarang = 0.0;

        for kata in teks.split(' ').filter(|kata| !kata.is_empty()) {
            let kata = self.encode(kata);
            let lebar_kata = self.lebar(&kata, ukuran);
            if !sekarang.is_empty() && lebar_sekarang + spasi + lebar_kata > maks {
                baris.push(std::mem::take(&mut sekarang));
                lebar_sekarang = 0.0;
            }
            if !sekarang.is_empty() {
                sekarang.push(b' ');
                lebar_sekarang += spasi;
            }
            sekarang.extend_from_slice(&kata);
            lebar_sekarang += lebar_kata;
        }
        if !sekarang.is_empty() {
            baris.push(sekarang);
        }
        baris
    }
}

/// WinAnsiEncoding (Windows-1252): Latin-1 except 0x80-0x9F, which hold typographic characters.
fn win_ansi(c: char) -> Option<u8> {
    let kode = u32::from(c);
    match kode {
        0x20..=0x7E | 0xA0..=0xFF => u8::try_from(kode).ok(),
        _ => {
            let kode = match c {
                '\u{20AC}' => 0x80,
                '\u{201A}' => 0x82,
                '\u{0192}' => 0x83,
                '\u{201E}' => 0x84,
                '\u{2026}' => 0x85,
                '\u{2020}' => 0x86,
                '\u{2021}' => 0x87,
                '\u{02C6}' => 0x88,
                '\u{2030}' => 0x89,
                '\u{0160}' => 0x8A,
                '\u{2039}' => 0x8B,
                '\u{0152}' => 0x8C,
                '\u{017D}' => 0x8E,
                '\u{2018}' => 0x91,
                '\u{2019}' => 0x92,
                '\u{201C}' => 0x93,
                '\u{201D}' => 0x94,
                '\u{2022}' => 0x95,
                '\u{2013}' => 0x96,
                '\u{2014}' => 0x97,
                '\u{02DC}' => 0x98,
                '\u{2122}' => 0x99,
                '\u{0161}' => 0x9A,
                '\u{203A}' => 0x9B,
                '\u{0153}' => 0x9C,
                '\u{017E}' => 0x9E,
                '\u{0178}' => 0x9F,
                _ => return None,
            };
            Some(kode)
        }
    }
}

#[rustfmt::skip]
const TIMES_ROMAN: [u16; 224] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278,
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444,
    921, 722, 667, 667, 722, 611, 556, 722, 722, 333, 389, 722, 611, 889, 722, 722,
    556, 722, 667, 556, 611, 722, 722, 944, 722, 722, 611, 333, 278, 333, 469, 500,
    333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778, 500, 500,
    500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541, 0,
    500, 0, 333, 500, 444, 1000, 500, 500, 333, 1000, 556, 333, 889, 0, 611, 0,
    0, 333, 333, 444, 444, 350, 500, 1000, 333, 980, 389, 333, 722, 0, 444, 722,
    250, 333, 500, 500, 500, 500, 200, 500, 333, 760, 276, 500, 564, 333, 760, 333,
    400, 564, 0, 0, 333, 500, 453, 250, 333, 0, 310, 500, 750, 750, 750, 444,
    722, 722, 722, 722, 722, 722, 889, 667, 611, 611, 611, 611, 333, 333, 333, 333,
    722, 722, 722, 722, 722, 722, 722, 564, 722, 722, 722, 722, 722, 722, 556, 500,
    444, 444, 444, 444, 444, 444, 667, 444, 444, 444, 444, 444, 278, 278, 278, 278,
    500, 500, 500, 500, 500, 500, 500, 564, 500, 500, 500, 500, 500, 500, 500, 500,
];

#[rustfmt::skip]
const TIMES_BOLD: [u16; 224] = [
    250, 333, 555, 500, 500, 1000, 833, 278, 333, 333, 500, 570, 250, 333, 250, 278,
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500,
    930, 722, 667, 722, 722, 667, 611, 778, 778, 389, 500, 778, 667, 944, 722, 778,
    611, 778, 722, 556, 667, 722, 722, 1000, 722, 722, 667, 333, 278, 333, 581, 500,
    333, 500, 556, 444, 556, 444, 333, 500, 556, 278, 333, 556, 278, 833, 556, 500,
    556, 556, 444, 389, 333, 556, 500, 722, 500, 500, 444, 394, 220, 394, 520, 0,
    500, 0, 333, 500, 500, 1000, 500, 500, 333, 1000, 556, 333, 1000, 0, 667, 0,
    0, 333, 333, 500, 500, 350, 500, 1000, 333, 1000, 389, 333, 722, 0, 444, 722,
    250, 333, 500, 500, 500, 500, 220, 500, 333, 747, 300, 500, 570, 333, 747, 333,
    400, 570, 0, 0, 333, 556, 540, 250, 333, 0, 330, 500, 750, 750, 750, 500,
    722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667, 389, 389, 389, 389,
    722, 722, 778, 778, 778, 778, 778, 570, 778, 722, 722, 722, 722, 722, 611, 556,
    500, 500, 500, 500, 500, 500, 722, 444, 444, 444, 444, 444, 278, 278, 278, 278,
    500, 556, 500, 500, 500, 500, 500, 570, 500, 556, 556, 556, 556, 500, 556, 500,
];

#[cfg(test)]
mod tests {
    use super::Huruf;

    #[test]
    fn widths_match_the_afm_files() {
        // "Hello" in Times-Roman: H 722, e 444, l 278, l 278, o 500.
        assert_eq!(Huruf::Biasa.lebar(b"Hello", 1000.0), 2222.0);
        // Bold is wider: H 778, e 444, l 278, l 278, o 500.
        assert_eq!(Huruf::Tebal.lebar(b"Hello", 1000.0), 2278.0);
    }

    #[test]
    fn encodes_latin_and_replaces_the_rest() {
        assert_eq!(
            Huruf::Biasa.encode("Andr\u{e9} \u{2013} \u{4e2d}"),
            b"Andr\xe9 \x96 ?"
        );
    }

    #[test]
    fn breaks_lines_at_spaces() {
        // At 12 pt: "satu dua" is 39.7 pt, "tiga empat" 50.3 pt, so 45 pt splits the latter.
        let baris = Huruf::Biasa.pecah_baris("satu dua tiga empat", 12.0, 45.0);
        assert_eq!(
            baris,
            vec![b"satu dua".to_vec(), b"tiga".to_vec(), b"empat".to_vec()]
        );
    }

    #[test]
    fn keeps_non_breaking_spaces_together() {
        let baris = Huruf::Biasa.pecah_baris("satu dua\u{a0}tiga", 12.0, 45.0);
        assert_eq!(baris, vec![b"satu".to_vec(), b"dua\xa0tiga".to_vec()]);
    }
}
