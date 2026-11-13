//! A picture and formatted text, between the forms Windows' clipboard holds them in and the forms the Mac's does.
//!
//! Remote Desktop's clipboard is Windows' clipboard sent over a channel ([`crate::rdp_clipboard`]): a picture is a
//! device-independent bitmap (a DIB) and formatted text is "HTML Format", a header of offsets before the HTML. On
//! Windows they pass through as bytes. The Mac's clipboard holds a picture as a PNG (or a TIFF, which the Mac turns
//! into a PNG itself) and formatted text as plain HTML, so [`crate::clipboard_contents`] converts each way with what's
//! here.
//!
//! Nothing here touches a clipboard: bytes in, bytes out, so the tests are the same on every system. It's built for
//! the Mac, the one system that uses it, and for the tests everywhere.

/// The size of a bitmap file's own header, which a DIB is a bitmap file without
const FILE_HEADER: usize = 14;
/// The size of the plain DIB header (BITMAPINFOHEADER), the one written here and the one every Windows program reads
const INFO_HEADER: u32 = 40;
/// A DIB's compression field when three colour masks follow a 40-byte header (BI_BITFIELDS)
const BITFIELDS: u32 = 3;

/// A picture as a DIB (as Windows' clipboard holds one, CF_DIB), from a PNG: a 40-byte header, then its pixels as
/// blue, green, red and alpha, the bottom row first. None when the PNG can't be read.
pub fn dib_from_png(png: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    // palettes, greys under 8 bits and 16-bit samples all come out as 8 bits a sample
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut pixels).ok()?;
    let (width, height) = (frame.width as usize, frame.height as usize);
    let samples = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let size = width.checked_mul(height)?.checked_mul(4)?;
    let mut dib = Vec::with_capacity(INFO_HEADER as usize + size);
    // the header: its size, width, height (positive: bottom row first), 1 plane, 32 bits, no compression, the
    // pixels' size, and nothing said of resolution or a palette
    for (value, bytes) in [
        (INFO_HEADER, 4),
        (u32::try_from(width).ok()?, 4),
        (u32::try_from(height).ok()?, 4),
        (1, 2),
        (32, 2),
        (0, 4),
        (u32::try_from(size).ok()?, 4),
        (0, 4),
        (0, 4),
        (0, 4),
        (0, 4),
    ] {
        dib.extend_from_slice(&value.to_le_bytes()[..bytes]);
    }
    for row in (0..height).rev() {
        let line = pixels.get(row * frame.line_size..row * frame.line_size + width * samples)?;
        for pixel in line.chunks_exact(samples) {
            let (red, green, blue, alpha) = match *pixel {
                [grey] => (grey, grey, grey, 0xff),
                [grey, alpha] => (grey, grey, grey, alpha),
                [red, green, blue] => (red, green, blue, 0xff),
                [red, green, blue, alpha] => (red, green, blue, alpha),
                _ => return None,
            };
            dib.extend_from_slice(&[blue, green, red, alpha]);
        }
    }
    Some(dib)
}

/// A DIB as a bitmap file (a .bmp): the same bytes after a 14-byte header saying how long the file is and where
/// its pixels start. The Mac reads a bitmap file in every form Windows writes one (any depth, a palette, colour
/// masks), so a DIB becomes a PNG there by way of this. None when the DIB is too short to have a header.
pub fn bmp_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
    let number = |at: usize, bytes: usize| -> Option<u32> {
        let mut value = [0u8; 4];
        value[..bytes].copy_from_slice(dib.get(at..at + bytes)?);
        Some(u32::from_le_bytes(value))
    };
    let header = number(0, 4)?;
    if header < INFO_HEADER {
        return None;
    }
    let (bits, compression, colours) = (number(14, 2)?, number(16, 4)?, number(32, 4)?);
    // between the header and the pixels: three colour masks after a plain header that says so (longer headers hold
    // their masks themselves), then the palette, which pictures of up to 8 bits always have
    let masks = if header == INFO_HEADER && compression == BITFIELDS { 12 } else { 0 };
    let palette = match (colours, bits) {
        (0, 1..=8) => 1u32 << bits,
        (colours, _) => colours,
    };
    let pixels = u32::try_from(FILE_HEADER).ok()?.checked_add(header)?.checked_add(masks)?.checked_add(palette.checked_mul(4)?)?;
    let length = u32::try_from(FILE_HEADER + dib.len()).ok()?;
    let mut bmp = Vec::with_capacity(FILE_HEADER + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&length.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&pixels.to_le_bytes());
    bmp.extend_from_slice(dib);
    Some(bmp)
}

/// The comments Windows programs put around the part of the HTML that was copied
const START_FRAGMENT: &str = "<!--StartFragment-->";
/// See [`START_FRAGMENT`]
const END_FRAGMENT: &str = "<!--EndFragment-->";

/// HTML as Windows' clipboard holds it ("HTML Format"): a header giving, in bytes from the start, where the HTML
/// begins and ends and where the part copied does, then the HTML with that part marked. The numbers are ten digits
/// each, so the header's length doesn't depend on them.
pub fn html_format(html: &str) -> Vec<u8> {
    let header = |start_html: usize, end_html: usize, start_fragment: usize, end_fragment: usize| {
        format!(
            "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
        )
    };
    let before = format!("<html><body>\r\n{START_FRAGMENT}");
    let after = format!("{END_FRAGMENT}\r\n</body></html>");
    let start_html = header(0, 0, 0, 0).len();
    let start_fragment = start_html + before.len();
    let end_fragment = start_fragment + html.len();
    let end_html = end_fragment + after.len();
    format!("{}{before}{html}{after}", header(start_html, end_html, start_fragment, end_fragment)).into_bytes()
}

/// The HTML in Windows' "HTML Format": the part that was copied where the header says where it is, else everything
/// after the header. None when there's no HTML to be found in it.
pub fn html_from_format(data: &[u8]) -> Option<String> {
    // Windows programs end it with a NUL, and some pad it
    let end = data.iter().position(|byte| *byte == 0).unwrap_or(data.len());
    let data = &data[..end];
    let text = String::from_utf8_lossy(data);
    // a number in the header, by its name; -1 is how a program says it gives none
    let offset = |name: &str| -> Option<usize> {
        let from = text.find(name)? + name.len();
        let digits: String = text[from..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    let part = |start: Option<usize>, end: Option<usize>| -> Option<String> {
        let part = data.get(start?..end?.min(data.len()))?;
        (!part.is_empty()).then(|| String::from_utf8_lossy(part).into_owned())
    };
    part(offset("StartFragment:"), offset("EndFragment:"))
        .or_else(|| part(offset("StartHTML:"), offset("EndHTML:")))
        .or_else(|| text.find('<').map(|at| text[at..].to_string()))
}

// The conversions alone, on bytes made here: no clipboard, so the same on Windows, the Mac and Linux.
#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG of these pixels (red, green, blue, alpha), `width` to a row
    fn png_of(width: u32, rgba: &[u8]) -> Vec<u8> {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, width, rgba.len() as u32 / 4 / width);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(rgba).unwrap();
        file
    }

    #[test]
    fn a_picture_copied_on_the_mac_is_the_bitmap_windows_pastes() {
        // two rows of two: red and green above, blue and half-clear white below
        let png = png_of(2, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128]);
        let dib = dib_from_png(&png).expect("the PNG is read");
        // the plain header: 40 bytes, 2 × 2, one plane, 32 bits, uncompressed, 16 bytes of pixels
        assert_eq!(&dib[..20], &[40, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0]);
        assert_eq!(&dib[20..24], &[16, 0, 0, 0]);
        // the bottom row first, each pixel blue, green, red, alpha
        assert_eq!(&dib[40..], &[255, 0, 0, 255, 255, 255, 255, 128, 0, 0, 255, 255, 0, 255, 0, 255]);
        assert_eq!(dib_from_png(b"not a picture"), None);
    }

    #[test]
    fn a_bitmap_copied_in_windows_is_a_bitmap_file_the_mac_reads() {
        let dib = dib_from_png(&png_of(1, &[1, 2, 3, 255])).unwrap();
        let bmp = bmp_from_dib(&dib).unwrap();
        // "BM", the file's length, and the pixels right after the two headers
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize, bmp.len());
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 54);
        assert_eq!(&bmp[14..], &dib[..]);
        // a screenshot as Windows copies one: colour masks after the header, then the pixels
        let mut masked = dib.clone();
        masked[16..20].copy_from_slice(&3u32.to_le_bytes());
        masked.splice(40..40, [0u8; 12]);
        assert_eq!(u32::from_le_bytes(bmp_from_dib(&masked).unwrap()[10..14].try_into().unwrap()), 66);
        // and a 16-colour picture: its palette comes before the pixels
        let mut few = dib.clone();
        few[14..16].copy_from_slice(&4u16.to_le_bytes());
        assert_eq!(u32::from_le_bytes(bmp_from_dib(&few).unwrap()[10..14].try_into().unwrap()), 54 + 16 * 4);
        assert_eq!(bmp_from_dib(&dib[..30]), None);
    }

    #[test]
    fn formatted_text_copied_on_the_mac_is_marked_as_windows_reads_it() {
        let html = "<b>bold</b> – ünïcode";
        let data = html_format(html);
        let text = String::from_utf8(data.clone()).unwrap();
        let offset = |name: &str| -> usize {
            let from = text.find(name).unwrap() + name.len();
            text[from..from + 10].parse().unwrap()
        };
        assert!(text.starts_with("Version:0.9\r\nStartHTML:"));
        // the offsets are in bytes, and point at what they say
        assert_eq!(&data[offset("StartFragment:")..offset("EndFragment:")], html.as_bytes());
        let whole = &text[offset("StartHTML:")..offset("EndHTML:")];
        assert!(whole.starts_with("<html>") && whole.ends_with("</html>"));
        assert!(whole.contains("<!--StartFragment--><b>bold</b> – ünïcode<!--EndFragment-->"));
        assert_eq!(offset("EndHTML:"), data.len());
    }

    #[test]
    fn formatted_text_copied_in_windows_is_the_html_the_mac_pastes() {
        // back from our own
        assert_eq!(html_from_format(&html_format("<i>italic</i> – é")).as_deref(), Some("<i>italic</i> – é"));
        // as a browser on Windows copies it, with a NUL after it
        let copied = b"Version:0.9\r\nStartHTML:0000000105\r\nEndHTML:0000000189\r\nStartFragment:0000000141\r\nEndFragment:0000000153\r\n<html>\r\n<body>\r\n<!--StartFragment--><u>under</u><!--EndFragment-->\r\n</body>\r\n</html>\0";
        assert_eq!(html_from_format(copied).as_deref(), Some("<u>under</u>"));
        // a header that gives no place for the part copied: the whole of the HTML
        assert_eq!(html_from_format(b"Version:1.0\r\nStartHTML:-1\r\nEndHTML:-1\r\n<p>all</p>").as_deref(), Some("<p>all</p>"));
        assert_eq!(html_from_format(b"Version:0.9\r\n"), None);
    }
}
