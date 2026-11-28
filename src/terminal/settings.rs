//! Persisted terminal preferences and one incremental decoder per transport stream.
use super::{Cell, ColorSource, Ink, PALETTE};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalFont {
    #[default]
    JetBrainsMonoNerd,
    DejaVuSansMono,
    LiberationSans,
    MonospaceFallback,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterEncoding {
    #[default]
    Utf8,
    Windows1252,
    ShiftJis,
}
impl CharacterEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Windows1252 => "Windows-1252",
            Self::ShiftJis => "Shift_JIS",
        }
    }
    fn encoding(self) -> &'static encoding_rs::Encoding {
        match self {
            Self::Utf8 => encoding_rs::UTF_8,
            Self::Windows1252 => encoding_rs::WINDOWS_1252,
            Self::ShiftJis => encoding_rs::SHIFT_JIS,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackspaceMapping {
    #[default]
    Delete,
    ControlH,
}
impl BackspaceMapping {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Delete => b"\x7f",
            Self::ControlH => b"\x08",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dimensions {
    #[default]
    Dynamic,
    Fixed {
        cols: u16,
        rows: u16,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalPalette {
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub ansi: [[u8; 3]; 16],
}
impl Default for TerminalPalette {
    fn default() -> Self {
        Self::dark()
    }
}
impl TerminalPalette {
    pub fn dark() -> Self {
        Self {
            foreground: [234, 242, 237],
            background: [19, 25, 23],
            ansi: PALETTE.map(|Ink(r, g, b)| [r, g, b]),
        }
    }
    pub fn light() -> Self {
        Self {
            foreground: [32, 32, 32],
            background: [251, 253, 252],
            ansi: PALETTE.map(|Ink(r, g, b)| [r, g, b]),
        }
    }
    pub fn colors(&self, cell: &Cell) -> (Ink, Ink) {
        let color = |source: ColorSource, rgb: Ink, default: [u8; 3]| match source {
            ColorSource::Default => Ink(default[0], default[1], default[2]),
            ColorSource::Indexed(index) if index < 16 => {
                let rgb = self.ansi[index as usize];
                Ink(rgb[0], rgb[1], rgb[2])
            }
            _ => rgb,
        };
        let mut colors = (
            color(cell.fg_source, cell.fg, self.foreground),
            color(cell.bg_source, cell.bg, self.background),
        );
        if cell.inverse {
            std::mem::swap(&mut colors.0, &mut colors.1);
        }
        colors
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    pub font_family: TerminalFont,
    pub font_size: f32,
    pub light_palette: TerminalPalette,
    pub dark_palette: TerminalPalette,
    pub encoding: CharacterEncoding,
    pub backspace: BackspaceMapping,
    pub dimensions: Dimensions,
}
impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: TerminalFont::default(),
            font_size: 14.,
            light_palette: TerminalPalette::light(),
            dark_palette: TerminalPalette::dark(),
            encoding: CharacterEncoding::Utf8,
            backspace: BackspaceMapping::Delete,
            dimensions: Dimensions::Dynamic,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalOverrides {
    pub font_family: Option<TerminalFont>,
    pub font_size: Option<f32>,
    pub light_palette: Option<TerminalPalette>,
    pub dark_palette: Option<TerminalPalette>,
    pub encoding: Option<CharacterEncoding>,
    pub backspace: Option<BackspaceMapping>,
    pub dimensions: Option<Dimensions>,
}
pub type ResolvedTerminalSettings = TerminalSettings;
impl TerminalOverrides {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
impl TerminalSettings {
    pub fn resolve(global: &Self, session: &TerminalOverrides) -> ResolvedTerminalSettings {
        let mut out = Self {
            font_family: session.font_family.unwrap_or(global.font_family),
            font_size: session.font_size.unwrap_or(global.font_size),
            light_palette: session
                .light_palette
                .clone()
                .unwrap_or_else(|| global.light_palette.clone()),
            dark_palette: session
                .dark_palette
                .clone()
                .unwrap_or_else(|| global.dark_palette.clone()),
            encoding: session.encoding.unwrap_or(global.encoding),
            backspace: session.backspace.unwrap_or(global.backspace),
            dimensions: session.dimensions.unwrap_or(global.dimensions),
        };
        out.font_size = if out.font_size.is_finite() {
            out.font_size.clamp(8., 32.)
        } else {
            14.
        };
        if let Dimensions::Fixed { cols, rows } = out.dimensions {
            out.dimensions = Dimensions::Fixed {
                cols: cols.clamp(2, 500),
                rows: rows.clamp(2, 200),
            };
        }
        out
    }
}
pub struct TerminalCodec {
    encoding: CharacterEncoding,
    decoder: encoding_rs::Decoder,
    bytes: u64,
    errors: u64,
    finished: bool,
}
impl TerminalCodec {
    pub fn new(encoding: CharacterEncoding) -> Self {
        Self {
            encoding,
            decoder: encoding.encoding().new_decoder_without_bom_handling(),
            bytes: 0,
            errors: 0,
            finished: false,
        }
    }
    pub fn source_bytes(&self) -> u64 {
        self.bytes
    }
    pub fn errors(&self) -> u64 {
        self.errors
    }
    pub fn encoding(&self) -> CharacterEncoding {
        self.encoding
    }
    fn decode(&mut self, bytes: &[u8], last: bool) -> String {
        let capacity = self
            .decoder
            .max_utf8_buffer_length(bytes.len())
            .unwrap_or(bytes.len().saturating_mul(3).saturating_add(16));
        let mut out = String::with_capacity(capacity);
        let mut offset = 0;
        loop {
            let (result, read, errors) =
                self.decoder
                    .decode_to_string(&bytes[offset..], &mut out, last);
            offset += read;
            if errors {
                self.errors += 1;
            }
            if result == encoding_rs::CoderResult::InputEmpty {
                break;
            }
            out.reserve(8192);
        }
        out
    }
    pub fn feed(&mut self, bytes: &[u8]) -> String {
        self.bytes = self.bytes.saturating_add(bytes.len() as u64);
        self.finished = false;
        self.decode(bytes, false)
    }
    pub fn finish(&mut self) -> String {
        if self.finished {
            return String::new();
        }
        let out = self.decode(&[], true);
        self.decoder = self.encoding.encoding().new_decoder_without_bom_handling();
        self.finished = true;
        out
    }
    pub fn set_encoding(&mut self, encoding: CharacterEncoding) -> String {
        if encoding == self.encoding {
            return String::new();
        }
        let out = self.finish();
        self.encoding = encoding;
        self.decoder = encoding.encoding().new_decoder_without_bom_handling();
        self.finished = false;
        out
    }
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, String> {
        let (bytes, _, errors) = self.encoding.encoding().encode(text);
        if errors {
            Err(format!(
                "Input contains characters unavailable in {}. Choose UTF-8 or change the text.",
                self.encoding.label()
            ))
        } else {
            Ok(bytes.into_owned())
        }
    }
}
