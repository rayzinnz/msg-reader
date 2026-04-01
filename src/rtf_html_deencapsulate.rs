//! RTF HTML De-encapsulation per MS-OXRTFEX specification
//!
//! This module extracts HTML content encapsulated within Outlook RTF messages.
//! According to MS-OXRTFEX (Outlook RTF External Content):
//!
//! ## Structure (Section 2.2.3)
//!
//! Outlook RTF with encapsulated HTML has this structure:
//!
//! ```text
//! {\rtf1\ansi\ansicpgN\fromhtml
//!   ... header info ...
//!   {\*\htmltagN <html>}
//!   {\*\htmltagN <head>}
//!   ... more HTML tags ...
//!   }\htmlrtf { RTF-only content }\htmlrtf0
//!   {\*\htmltagN <body>}
//!   }\htmlrtf { RTF-only }\htmlrtf0 Text content {\*\htmltagN <span>} more text
//!   {\*\htmltagN </body>}
//!   {\*\htmltagN </html>}
//! }
//! ```
//!
//! ## Key Control Words
//!
//! - `\fromhtml` - Marks this RTF as containing encapsulated HTML
//! - `{\*\htmltagN ...}` - Destination group containing HTML tags (N = group number)
//! - `\htmlrtf` - Begin RTF-only content (ignored by HTML processors)
//! - `\htmlrtf0` - End RTF-only content, begin HTML content
//! - `\htmlrtfN` where N>0 - Begin RTF-only content with nesting level
//!
//! ## Text Content Location
//!
//! The actual document text appears:
//! 1. Outside of `{\*\htmltagN}` groups
//! 2. After `\htmlrtf0` control word (HTML content mode)
//! 3. Before the next `{\*\htmltagN}` group or `\htmlrtf` control word
//!
//! # RTF Escape Sequences (MS-OXRTFEX 2.2.3.2)
//!
//! - `\'hh` - Hex byte escape (hh = two hex digits)
//! - `\uN` - Unicode character (N = signed decimal code point)
//! - `\ucN` - Sets fallback byte count for subsequent `\uN` escapes
//! - `\\`, `\{`, `\}` - Escaped literal characters
//! - `\par` - Paragraph break (typically becomes `<br>` or preserved as RTF)

use anyhow::{Result, bail};
use encoding_rs::{Encoding, WINDOWS_1252};

/// State for tracking HTML vs RTF content mode
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ContentMode {
    /// Haven't started outputting yet - waiting for first htmltag or \htmlrtf0
    Waiting,
    /// Content should be included in HTML output
    Html,
    /// Content is RTF-only, skip for HTML output
    Rtf,
}

/// Parser state for RTF HTML de-encapsulation
struct RtfHtmlParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    encoding: &'static Encoding,
    output: String,
    mode: ContentMode,
    uc: i32, // Fallback byte count for \uN escapes
}

/// Extract HTML from Outlook RTF per MS-OXRTFEX specification.
///
/// This function:
/// 1. Validates the RTF contains `\fromhtml` marker (MS-OXRTFEX 2.2.3.1)
/// 2. Determines the codepage from `\ansicpgN` control word
/// 3. Processes the RTF stream, handling:
///    - `{\*\htmltagN ...}` groups: Extract HTML tags
///    - `\htmlrtf` / `\htmlrtf0`: Switch between RTF and HTML modes
///    - Text content outside groups: Decode and include in HTML
/// 4. Returns the concatenated HTML string
///
/// # Arguments
///
/// * `rtf` - The raw RTF string from the MSG file's PR_BODY_HTML or PR_RTF_COMPRESSED stream
///
/// # Returns
///
/// * `Some(String)` - The extracted and decoded HTML content
/// * `None` - If no valid HTML encapsulation is found
pub fn rtf_to_html_outlook(rtf: &str) -> Result<String> {
    // Validate RTF structure - must begin with '{\rtf'
    if !rtf.starts_with("{\\rtf") {
        bail!("Not an rtf document")
    }

    // Check for \fromhtml marker (MS-OXRTFEX 2.2.3.1)
    // The marker should appear in the header
    let header_end = rtf.find("{\\*\\htmltag").unwrap_or(rtf.len().min(4096));
    let header = &rtf[..header_end];
    if !header.contains("\\fromhtml") {
        // Not an Outlook HTML-encapsulated RTF
        bail!("Not a '\\fromhtml' MS-OXRTFEX rtf document")
    }

    // Determine codepage from \ansicpgN control word
    let codepage = parse_ansicpg(header).unwrap_or(1252);
    let encoding = encoding_from_codepage(codepage).unwrap_or(WINDOWS_1252);

    // Parse the RTF stream
    let mut parser = RtfHtmlParser {
        bytes: rtf.as_bytes(),
        pos: 0,
        encoding,
        output: String::new(),
        mode: ContentMode::Waiting, // Wait for first htmltag before outputting
        uc: 1, // Default fallback count
    };

    parser.parse();

    if parser.output.is_empty() {
        bail!("No parser output")
    } else {
        Ok(parser.output)
    }
}

impl<'a> RtfHtmlParser<'a> {
    /// Main parsing loop
    fn parse(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'{' => self.handle_open_brace(),
                b'}' => self.handle_close_brace(),
                b'\\' => self.handle_backslash(),
                _ => self.handle_text_char(),
            }
        }
    }

    /// Handle opening brace - check for {\*\htmltagN} groups
    fn handle_open_brace(&mut self) {
        // Check if this is a {\*\htmltagN} group
        if self.bytes[self.pos..].starts_with(b"{\\*\\htmltag") {
            // Transition from Waiting to Html mode on first htmltag
            if self.mode == ContentMode::Waiting {
                self.mode = ContentMode::Html;
            }
            self.extract_htmltag_group();
        } else {
            // Regular group - skip the opening brace
            self.pos += 1;
        }
    }

    /// Handle closing brace
    fn handle_close_brace(&mut self) {
        self.pos += 1;
    }

    /// Handle backslash - control word or escape sequence
    fn handle_backslash(&mut self) {
        self.pos += 1; // Skip backslash
        if self.pos >= self.bytes.len() {
            return;
        }

        let ch = self.bytes[self.pos];

        // Check for \htmlrtf, \htmlrtf0 control words
        if self.bytes[self.pos..].starts_with(b"htmlrtf") {
            self.pos += 7; // Skip "htmlrtf"
            // Check for parameter (0 or positive number)
            if self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                let num_start = self.pos;
                while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
                let param: u32 = std::str::from_utf8(&self.bytes[num_start..self.pos])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                // \htmlrtf0 means switch to HTML mode, \htmlrtf or \htmlrtfN (N>0) means RTF mode
                // Also transitions from Waiting to Html mode
                self.mode = if param == 0 { ContentMode::Html } else { ContentMode::Rtf };
            } else {
                // \htmlrtf without parameter = RTF mode
                self.mode = ContentMode::Rtf;
            }
            // Skip optional space delimiter
            if self.pos < self.bytes.len() && self.bytes[self.pos] == b' ' {
                self.pos += 1;
            }
            return;
        }

        // Escaped literal: \\ \{ \}
        if ch == b'\\' || ch == b'{' || ch == b'}' {
            if self.mode == ContentMode::Html {
                self.output.push(ch as char);
            }
            self.pos += 1;
            return;
        }

        // Hex escape: \'hh
        if ch == b'\'' && self.pos + 2 < self.bytes.len() {
            if let Some(byte) = parse_hex_byte(self.bytes[self.pos + 1], self.bytes[self.pos + 2]) {
                if self.mode == ContentMode::Html {
                    let byte_array = [byte];
                    let (cow, _, _) = self.encoding.decode(&byte_array);
                    self.output.push_str(&cow);
                }
            }
            self.pos += 3;
            return;
        }

        // Unicode escape: \uN
        if ch == b'u' {
            self.pos += 1;
            let negative = self.pos < self.bytes.len() && self.bytes[self.pos] == b'-';
            if negative {
                self.pos += 1;
            }
            let mut codepoint: i32 = 0;
            let mut has_digits = false;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                codepoint = codepoint * 10 + (self.bytes[self.pos] - b'0') as i32;
                has_digits = true;
                self.pos += 1;
            }
            if has_digits {
                if negative {
                    codepoint = -codepoint;
                }
                if self.mode == ContentMode::Html {
                    if let Some(c) = char::from_u32(codepoint as u32) {
                        self.output.push(c);
                    }
                }
                // Skip fallback bytes
                for _ in 0..self.uc.max(0) {
                    if self.pos >= self.bytes.len() {
                        break;
                    }
                    if self.bytes[self.pos] == b'\\' {
                        self.pos += 1;
                        if self.pos < self.bytes.len() {
                            self.pos = skip_rtf_escape(self.bytes, self.pos);
                        }
                    } else {
                        self.pos += 1;
                    }
                }
            }
            // Skip optional space delimiter
            if self.pos < self.bytes.len() && self.bytes[self.pos] == b' ' {
                self.pos += 1;
            }
            return;
        }

        // Control word: \ucN or other
        if ch.is_ascii_alphabetic() {
            let word_start = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_alphabetic() {
                self.pos += 1;
            }
            let word = &self.bytes[word_start..self.pos];

            // Parse optional parameter
            let negative = self.pos < self.bytes.len() && self.bytes[self.pos] == b'-';
            if negative {
                self.pos += 1;
            }
            let mut param: i32 = 0;
            let mut has_digits = false;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                param = param * 10 + (self.bytes[self.pos] - b'0') as i32;
                has_digits = true;
                self.pos += 1;
            }
            if negative {
                param = -param;
            }

            // Optional space delimiter
            if self.pos < self.bytes.len() && self.bytes[self.pos] == b' ' {
                self.pos += 1;
            }

            // Handle \ucN - set fallback byte count
            if word == b"uc" && has_digits {
                self.uc = param;
            }
            // All other control words are ignored in HTML content mode
            return;
        }

        // Unknown escape - skip
        self.pos += 1;
    }

    /// Handle regular text character
    fn handle_text_char(&mut self) {
        if self.mode == ContentMode::Html {
            let ch = self.bytes[self.pos];
            // Pass through printable ASCII
            if ch >= 0x20 && ch <= 0x7E {
                self.output.push(ch as char);
            } else if ch == b'\t' || ch == b'\n' || ch == b'\r' {
                // Preserve whitespace
                self.output.push(ch as char);
            }
            // Other bytes are ignored (likely part of multi-byte sequences handled by escapes)
        }
        self.pos += 1;
    }

    /// Extract content from a {\*\htmltagN ...} group
    fn extract_htmltag_group(&mut self) {
        // Find the matching closing brace
        if let Some((group_bytes, next_pos)) = extract_balanced_group(self.bytes, self.pos) {
            // Extract the HTML content from inside the group
            let content = extract_htmltag_content(group_bytes);
            // Decode RTF escapes and append to output
            let decoded = decode_rtf_escapes(content, self.encoding);
            self.output.push_str(&decoded);
            self.pos = next_pos;
        } else {
            // Malformed group - skip the opening brace and continue
            self.pos += 1;
        }
    }
}

/// Parse the \ansicpgN control word to get the ANSI codepage number.
fn parse_ansicpg(rtf_header: &str) -> Option<u16> {
    let bytes = rtf_header.as_bytes();
    let needle = b"\\ansicpg";
    
    let mut i = 0;
    while let Some(offset) = bytes[i..].windows(needle.len()).position(|w| w == needle) {
        let start = i + offset + needle.len();
        if start < bytes.len() {
            let mut num_str = String::new();
            let mut j = start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                num_str.push(bytes[j] as char);
                j += 1;
            }
            if !num_str.is_empty() {
                if let Ok(cp) = num_str.parse::<u16>() {
                    return Some(cp);
                }
            }
        }
        i = start + 1;
    }
    None
}

/// Get the encoding for a given codepage number.
fn encoding_from_codepage(codepage: u16) -> Option<&'static Encoding> {
    match codepage {
        1252 => Some(WINDOWS_1252),
        1250 => Some(encoding_rs::WINDOWS_1250),
        1251 => Some(encoding_rs::WINDOWS_1251),
        1253 => Some(encoding_rs::WINDOWS_1253),
        1254 => Some(encoding_rs::WINDOWS_1254),
        1255 => Some(encoding_rs::WINDOWS_1255),
        1256 => Some(encoding_rs::WINDOWS_1256),
        1257 => Some(encoding_rs::WINDOWS_1257),
        1258 => Some(encoding_rs::WINDOWS_1258),
        932  => Some(encoding_rs::SHIFT_JIS),
        936  => Some(encoding_rs::GBK),
        949  => Some(encoding_rs::EUC_KR),
        950  => Some(encoding_rs::BIG5),
        874  => Some(encoding_rs::WINDOWS_874),
        _    => None
    }
}

/// Extract a balanced RTF group starting at the given position.
fn extract_balanced_group(buf: &[u8], start: usize) -> Option<(&[u8], usize)> {
    if buf.get(start) != Some(&b'{') {
        return None;
    }

    let mut depth = 0usize;
    let mut i = start;

    while i < buf.len() {
        match buf[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.checked_sub(1)?;
                i += 1;
                if depth == 0 {
                    return Some((&buf[start..i], i));
                }
            }
            b'\\' => {
                i += 1;
                if i >= buf.len() {
                    break;
                }
                i = skip_rtf_escape(buf, i);
            }
            _ => {
                i += 1;
            }
        }
    }

    None
}

/// Skip an RTF escape sequence and return the position after it.
fn skip_rtf_escape(buf: &[u8], mut i: usize) -> usize {
    if i >= buf.len() {
        return i;
    }

    let ch = buf[i];

    // Hex escape \'hh
    if ch == b'\'' {
        return i + 3;
    }

    // Unicode escape \uN
    if ch == b'u' {
        i += 1;
        if i < buf.len() && buf[i] == b'-' {
            i += 1;
        }
        while i < buf.len() && buf[i].is_ascii_digit() {
            i += 1;
        }
        if i < buf.len() && buf[i] == b' ' {
            i += 1;
        }
        return i;
    }

    // Control word
    if ch.is_ascii_alphabetic() {
        while i < buf.len() && buf[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i < buf.len() && buf[i] == b'-' {
            i += 1;
        }
        while i < buf.len() && buf[i].is_ascii_digit() {
            i += 1;
        }
        if i < buf.len() && buf[i] == b' ' {
            i += 1;
        }
        return i;
    }

    // Single character escape
    i + 1
}

/// Extract the content from a `{\*\htmltagN ...}` group.
fn extract_htmltag_content(group: &[u8]) -> &[u8] {
    if group.len() < 2 {
        return &[];
    }

    // Strip outer braces
    let inner = &group[1..group.len() - 1];

    // Find and skip the `{\*\htmltagN` prefix
    let prefix = b"\\*\\htmltag";
    
    if let Some(pos) = inner.windows(prefix.len()).position(|w| w == prefix) {
        let mut i = pos + prefix.len();
        // Skip the group number digits
        while i < inner.len() && inner[i].is_ascii_digit() {
            i += 1;
        }
        // Skip optional space delimiter
        if i < inner.len() && inner[i] == b' ' {
            i += 1;
        }
        return &inner[i..];
    }

    inner
}

/// Decode RTF escape sequences in HTML content bytes.
fn decode_rtf_escapes(content: &[u8], encoding: &'static Encoding) -> String {
    let mut result = String::new();
    let mut uc = 1i32;
    let mut i = 0usize;

    while i < content.len() {
        if content[i] != b'\\' {
            // Pass through printable ASCII
            if content[i] >= 0x20 && content[i] <= 0x7E {
                result.push(content[i] as char);
            }
            i += 1;
            continue;
        }

        i += 1;
        if i >= content.len() {
            break;
        }

        let ch = content[i];

        // Literal escapes
        if ch == b'\\' || ch == b'{' || ch == b'}' {
            result.push(ch as char);
            i += 1;
            continue;
        }

        // Hex byte escape
        if ch == b'\'' && i + 2 < content.len() {
            if let Some(byte) = parse_hex_byte(content[i + 1], content[i + 2]) {
                let byte_array = [byte];
                let (cow, _, _) = encoding.decode(&byte_array);
                result.push_str(&cow);
            }
            i += 3;
            continue;
        }

        // Unicode escape
        if ch == b'u' {
            i += 1;
            let negative = i < content.len() && content[i] == b'-';
            if negative {
                i += 1;
            }
            let mut codepoint: i32 = 0;
            let mut has_digits = false;
            while i < content.len() && content[i].is_ascii_digit() {
                codepoint = codepoint * 10 + (content[i] - b'0') as i32;
                has_digits = true;
                i += 1;
            }
            if has_digits {
                if negative {
                    codepoint = -codepoint;
                }
                if let Some(c) = char::from_u32(codepoint as u32) {
                    result.push(c);
                }
                // Skip fallback bytes
                for _ in 0..uc.max(0) {
                    if i >= content.len() {
                        break;
                    }
                    if content[i] == b'\\' {
                        i += 1;
                        if i < content.len() {
                            i = skip_rtf_escape(content, i);
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            continue;
        }

        // Control word
        if ch.is_ascii_alphabetic() {
            let word_start = i;
            while i < content.len() && content[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word = &content[word_start..i];

            let negative = i < content.len() && content[i] == b'-';
            if negative {
                i += 1;
            }
            let mut param: i32 = 0;
            let mut has_digits = false;
            while i < content.len() && content[i].is_ascii_digit() {
                param = param * 10 + (content[i] - b'0') as i32;
                has_digits = true;
                i += 1;
            }
            if negative {
                param = -param;
            }

            if i < content.len() && content[i] == b' ' {
                i += 1;
            }

            if word == b"uc" && has_digits {
                uc = param;
            }
            continue;
        }

        i += 1;
    }

    result
}

/// Parse two hex digits into a byte value.
fn parse_hex_byte(high: u8, low: u8) -> Option<u8> {
    let h = hex_digit_to_value(high)?;
    let l = hex_digit_to_value(low)?;
    Some((h << 4) | l)
}

/// Convert a hex digit character to its numeric value.
fn hex_digit_to_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_htmltag() {
        let rtf = "{\\rtf1\\ansi\\ansicpg1252\\fromhtml{\\*\\htmltag1 <html>}}";
        let result = rtf_to_html_outlook(rtf).unwrap();
        assert_eq!(result, "<html>".to_string());
    }

    #[test]
    fn test_text_outside_htmltag() {
        // Text content appears outside htmltag groups, after \htmlrtf0
        let rtf = "{\\rtf1\\ansi\\ansicpg1252\\fromhtml{\\*\\htmltag1 <body>}\\htmlrtf0 Hello World{\\*\\htmltag2 </body>}}";
        let result = rtf_to_html_outlook(rtf).unwrap();
        assert_eq!(result, "<body>Hello World</body>".to_string());
    }

    #[test]
    fn test_htmlrtf_mode_switch() {
        // \htmlrtf switches to RTF-only mode (content skipped)
        // \htmlrtf0 switches back to HTML mode (content included)
        let rtf = "{\\rtf1\\ansi\\ansicpg1252\\fromhtml{\\*\\htmltag1 <p>}\\htmlrtf {\\fs24 RTF-only }\\htmlrtf0 HTML content{\\*\\htmltag2 </p>}}";
        let result = rtf_to_html_outlook(rtf).unwrap();
        assert_eq!(result, "<p>HTML content</p>".to_string());
    }

    #[test]
    fn test_full_example_structure() {
        // Simplified version of the example from vibe.md
        let rtf = "{\\rtf1\\ansi\\ansicpg1252\\fromhtml
{\\*\\htmltag1 <html>}
{\\*\\htmltag2 <body>}
}\\htmlrtf {\\fs48 \\par\\b }\\htmlrtf0
{\\*\\htmltag3 <h1>}\\htmlrtf {\\htmlrtf0
A Heading
{\\*\\htmltag4 </h1>}
{\\*\\htmltag5 <p>}\\htmlrtf {\\htmlrtf0
Some normal text
{\\*\\htmltag6 </p>}
{\\*\\htmltag7 </body>}
{\\*\\htmltag8 </html>}}";
        let result = rtf_to_html_outlook(rtf);
        let html = result.expect("Should extract HTML");
        assert!(html.contains("<html>"));
        assert!(html.contains("<body>"));
        assert!(html.contains("A Heading"));
        assert!(html.contains("Some normal text"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_header_content_skipped() {
        // Font table and other header content should not appear in output
        let rtf = "{\\rtf1\\ansi\\ansicpg1252\\fromhtml{\\fonttbl{\\f0 Arial;}{\\f1 Courier;}}{\\*\\htmltag1 <html>}\\htmlrtf0 Hello{\\*\\htmltag2 </html>}}";
        let result = rtf_to_html_outlook(rtf);
        let html = result.expect("Should extract HTML");
        assert!(!html.contains("Arial"));
        assert!(!html.contains("Courier"));
        assert!(html.contains("<html>"));
        assert!(html.contains("Hello"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_hex_escape() {
        let rtf = "{\\rtf1\\ansi\\ansicpg1252\\fromhtml{\\*\\htmltag1 caf\\'E9}}";
        let result = rtf_to_html_outlook(rtf).unwrap();
        assert_eq!(result, "café".to_string());
    }

    #[test]
    fn test_no_fromhtml() {
        let rtf = "{\\rtf1\\ansi{\\*\\htmltag1 <html>}}";
        let result = rtf_to_html_outlook(rtf);
        assert!(result.is_err());
    }
}
