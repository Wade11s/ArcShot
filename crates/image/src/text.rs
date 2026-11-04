//! Deterministic text rasterization input shared by preview and output.
//!
//! The compositor draws text from a [`TextFont`] so the golden tests can supply
//! a reproducible built-in font while the native macOS system-font path can be
//! plugged in later without a second rasterizer. [`BitmapFont`] is a fixed 5x7
//! monospace face; unknown characters render as a hollow box, so every input is
//! deterministic and no glyph is ever silently dropped.

use arcshot_core::TextStyle;

/// One rasterized text box: coverage alpha plus white RGB, in physical pixels.
///
/// The bitmap's top-left is the text box top-left. Coverage is binary (0 or 255)
/// for the built-in font; the compositor applies the style colour.
#[derive(Clone, Debug, PartialEq)]
pub struct TextGlyphs {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl TextGlyphs {
    /// Admits an exact coverage surface after checking its byte length.
    ///
    /// # Errors
    /// Returns `None` when the byte length does not match `width * height * 4`.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            return None;
        }
        Some(Self {
            width,
            height,
            rgba,
        })
    }

    /// Physical width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }
    /// Physical height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }
    /// White RGB with coverage in the alpha channel, row-major.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Logical text metrics used by the app to size and position the text box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    /// Number of lines after splitting on `\n`.
    pub line_count: usize,
    /// Widest line in logical points, using the monospace cell advance.
    pub max_line_width: f64,
    /// Logical line height.
    pub line_height: f64,
}

impl TextMetrics {
    /// The logical bounds of a text box whose anchor is the top-left.
    #[must_use]
    pub fn bounds(&self, anchor: [f64; 2]) -> [f64; 4] {
        [
            anchor[0],
            anchor[1],
            self.max_line_width,
            self.line_count as f64 * self.line_height,
        ]
    }
}

/// Measures multi-line text in logical points with the built-in font metrics.
///
/// The text never wraps: each `\n` starts a new line and the width is the widest
/// line's monospace advance.
#[must_use]
pub fn measure_text(content: &str, size_points: f64) -> TextMetrics {
    let line_count = content.split('\n').count();
    let max_chars = content
        .split('\n')
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let unit = size_points / 8.0;
    TextMetrics {
        line_count,
        max_line_width: 6.0 * unit * max_chars as f64,
        line_height: size_points,
    }
}

/// A font input the shared compositor can rasterize text with.
pub trait TextFont: Send + Sync {
    /// Rasterizes one multi-line text box at a physical pixel size.
    ///
    /// `size_points` is the logical font size and `scale` is the physical pixels
    /// per logical point. Returns `None` when the size is not renderable.
    fn rasterize(&self, content: &str, size_points: f64, scale: f64) -> Option<TextGlyphs>;
}

/// The reproducible built-in 5x7 monospace font.
#[derive(Clone, Copy, Debug, Default)]
pub struct BitmapFont;

impl TextFont for BitmapFont {
    fn rasterize(&self, content: &str, size_points: f64, scale: f64) -> Option<TextGlyphs> {
        let size_px = size_points * scale;
        if !size_px.is_finite() || size_px <= 0.0 {
            return None;
        }
        let lines: Vec<&str> = content.split('\n').collect();
        let unit = size_px / 8.0;
        let advance = (6.0 * unit).round().max(1.0);
        let glyph_w = (5.0 * unit).round().max(1.0);
        let glyph_h = (7.0 * unit).round().max(1.0);
        let line_height = size_px.round().max(1.0);
        let max_chars = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        let width = (max_chars as f64 * advance).round().max(1.0) as u32;
        let height = (lines.len() as f64 * line_height).round().max(1.0) as u32;
        let mut rgba = vec![0_u8; width as usize * height as usize * 4];
        for (line_index, line) in lines.iter().enumerate() {
            let baseline = line_index as f64 * line_height + glyph_h;
            for (column, character) in line.chars().enumerate() {
                let bits = glyph_bits(character);
                let x0 = column as f64 * advance;
                let y0 = baseline - glyph_h;
                for dy in 0..glyph_h as i64 {
                    for dx in 0..glyph_w as i64 {
                        let source_x = (dx as f64 * 5.0 / glyph_w) as usize;
                        let source_y = (dy as f64 * 7.0 / glyph_h) as usize;
                        if !glyph_bit(bits, source_y, source_x) {
                            continue;
                        }
                        let x = (x0 + dx as f64).round() as i64;
                        let y = (y0 + dy as f64).round() as i64;
                        if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
                            continue;
                        }
                        let index = (y as usize * width as usize + x as usize) * 4;
                        rgba[index] = 255;
                        rgba[index + 1] = 255;
                        rgba[index + 2] = 255;
                        rgba[index + 3] = 255;
                    }
                }
            }
        }
        Some(TextGlyphs {
            width,
            height,
            rgba,
        })
    }
}

/// Packs seven 5-bit rows into one `u32`.
const fn pack(rows: [u8; 7]) -> u64 {
    let mut value = 0;
    let mut row = 0;
    while row < 7 {
        value |= (rows[row] as u64) << (row * 5);
        row += 1;
    }
    value
}

const fn glyph_bit(bits: u64, row: usize, column: usize) -> bool {
    (bits >> (row * 5 + column)) & 1 == 1
}

/// The hollow box fallback for characters outside the built-in face.
const UNKNOWN: u64 = pack([
    0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111,
]);

/// Returns the fixed 5x7 bitmap for one character.
fn glyph_bits(character: char) -> u64 {
    match character {
        ' ' => pack([0, 0, 0, 0, 0, 0, 0]),
        '!' => pack([0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100]),
        '"' => pack([0b01010, 0b01010, 0b01010, 0, 0, 0, 0]),
        '#' => pack([
            0b01010, 0b11111, 0b01010, 0b01010, 0b01010, 0b11111, 0b01010,
        ]),
        '%' => pack([
            0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011,
        ]),
        '&' => pack([
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ]),
        '\'' => pack([0b00100, 0b00100, 0b01000, 0, 0, 0, 0]),
        '(' => pack([
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ]),
        ')' => pack([
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ]),
        '*' => pack([0, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0]),
        '+' => pack([0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0]),
        ',' => pack([0, 0, 0, 0, 0b01100, 0b00100, 0b01000]),
        '-' => pack([0, 0, 0, 0b11111, 0, 0, 0]),
        '.' => pack([0, 0, 0, 0, 0, 0b01100, 0b01100]),
        '/' => pack([
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ]),
        '0' => pack([
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ]),
        '1' => pack([
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ]),
        '2' => pack([
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ]),
        '3' => pack([
            0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
        ]),
        '4' => pack([
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ]),
        '5' => pack([
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ]),
        '6' => pack([
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ]),
        '7' => pack([
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ]),
        '8' => pack([
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ]),
        '9' => pack([
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ]),
        ':' => pack([0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0]),
        ';' => pack([0, 0b01100, 0b01100, 0, 0b01100, 0b00100, 0b01000]),
        '<' => pack([
            0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010,
        ]),
        '=' => pack([0, 0, 0b11111, 0, 0b11111, 0, 0]),
        '>' => pack([
            0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000,
        ]),
        '?' => pack([0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100]),
        '@' => pack([
            0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110,
        ]),
        'A' => pack([
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ]),
        'B' => pack([
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ]),
        'C' => pack([
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ]),
        'D' => pack([
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ]),
        'E' => pack([
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ]),
        'F' => pack([
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ]),
        'G' => pack([
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ]),
        'H' => pack([
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ]),
        'I' => pack([
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ]),
        'J' => pack([
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ]),
        'K' => pack([
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ]),
        'L' => pack([
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ]),
        'M' => pack([
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ]),
        'N' => pack([
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ]),
        'O' => pack([
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ]),
        'P' => pack([
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ]),
        'Q' => pack([
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ]),
        'R' => pack([
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ]),
        'S' => pack([
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ]),
        'T' => pack([
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ]),
        'U' => pack([
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ]),
        'V' => pack([
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ]),
        'W' => pack([
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ]),
        'X' => pack([
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ]),
        'Y' => pack([
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ]),
        'Z' => pack([
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ]),
        '[' => pack([
            0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
        ]),
        '\\' => pack([
            0b10000, 0b01000, 0b01000, 0b00100, 0b00010, 0b00010, 0b00001,
        ]),
        ']' => pack([
            0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
        ]),
        '^' => pack([0b00100, 0b01010, 0b10001, 0, 0, 0, 0]),
        '_' => pack([0, 0, 0, 0, 0, 0, 0b11111]),
        '`' => pack([0b01000, 0b00100, 0, 0, 0, 0, 0]),
        'a' => pack([0, 0, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111]),
        'b' => pack([
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110,
        ]),
        'c' => pack([0, 0, 0b01110, 0b10000, 0b10000, 0b10001, 0b01110]),
        'd' => pack([
            0b00001, 0b00001, 0b01111, 0b10001, 0b10001, 0b10001, 0b01111,
        ]),
        'e' => pack([0, 0, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110]),
        'f' => pack([
            0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000,
        ]),
        'g' => pack([0, 0, 0b01111, 0b10001, 0b10001, 0b01111, 0b00001]),
        'h' => pack([
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001,
        ]),
        'i' => pack([0b00100, 0, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110]),
        'j' => pack([0b00010, 0, 0b00110, 0b00010, 0b00010, 0b10010, 0b01100]),
        'k' => pack([
            0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ]),
        'l' => pack([
            0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ]),
        'm' => pack([0, 0, 0b11010, 0b10101, 0b10101, 0b10101, 0b10101]),
        'n' => pack([0, 0, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001]),
        'o' => pack([0, 0, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110]),
        'p' => pack([0, 0, 0b11110, 0b10001, 0b11110, 0b10000, 0b10000]),
        'q' => pack([0, 0, 0b01111, 0b10001, 0b01111, 0b00001, 0b00001]),
        'r' => pack([0, 0, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000]),
        's' => pack([0, 0, 0b01111, 0b10000, 0b01110, 0b00001, 0b11110]),
        't' => pack([
            0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110,
        ]),
        'u' => pack([0, 0, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101]),
        'v' => pack([0, 0, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
        'w' => pack([0, 0, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010]),
        'x' => pack([0, 0, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001]),
        'y' => pack([0, 0, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110]),
        'z' => pack([0, 0, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111]),
        '{' => pack([
            0b00010, 0b00100, 0b00100, 0b01000, 0b00100, 0b00100, 0b00010,
        ]),
        '|' => pack([
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ]),
        '}' => pack([
            0b01000, 0b00100, 0b00100, 0b00010, 0b00100, 0b00100, 0b01000,
        ]),
        '~' => pack([0, 0, 0b01001, 0b10110, 0, 0, 0]),
        _ => UNKNOWN,
    }
}

/// Applies one text style's colour to a coverage mask for compositing.
///
/// The returned RGBA bytes are the style colour where coverage is opaque and
/// fully transparent elsewhere, so the compositor can blit them directly.
#[must_use]
pub fn tint(glyphs: &TextGlyphs, style: TextStyle) -> Vec<u8> {
    let [red, green, blue] = style.color().rgb();
    let mut tinted = Vec::with_capacity(glyphs.rgba.len());
    for pixel in glyphs.rgba.as_chunks::<4>().0 {
        if pixel[3] == 0 {
            tinted.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            tinted.extend_from_slice(&[red, green, blue, 255]);
        }
    }
    tinted
}
