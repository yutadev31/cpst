use arboard::{Clipboard, ImageData};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use std::io::{self, IsTerminal, Read, Write};

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod daemon;
#[cfg(unix)]
mod osc52;

/// A clipboard operation failure, retaining its underlying cause when available.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A local I/O, image, or clipboard operation failed.
    #[error("{operation}: {source}")]
    Operation {
        operation: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// Input is neither UTF-8 text nor a PNG, JPEG, or WebP image.
    #[error("stdin is neither UTF-8 text nor a PNG, JPEG, or WebP image")]
    InvalidText(#[source] std::str::Utf8Error),
    /// The provider did not confirm ownership before the deadline.
    #[error("timed out waiting to confirm clipboard copy{detail}", detail = .0.as_ref().map(|error| format!(": {error}")).unwrap_or_default())]
    ConfirmationTimeout(#[source] Option<arboard::Error>),
    /// The provider reported a protocol or lifecycle failure.
    #[error("{0}")]
    Daemon(String),
}

impl Error {
    fn operation(
        operation: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Operation {
            operation,
            source: Box::new(source),
        }
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::Daemon(message.to_owned())
    }
}

pub const DAEMON_ARGUMENT: &str = "__internal_daemonize";

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_SIGNATURE: &[u8] = b"\xff\xd8\xff";

/// Selects the content type and image encoding.
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum Format {
    #[default]
    Auto,
    Text,
    Image,
    Png,
    #[value(alias = "jpg")]
    Jpeg,
    Webp,
}

impl Format {
    fn requires_image(self) -> bool {
        !matches!(self, Self::Auto | Self::Text)
    }
}

enum Content {
    Text(String),
    Image(ImageData<'static>),
}

/// Outside Windows and macOS, `daemon_executable` must handle [`DAEMON_ARGUMENT`] by calling [`run_daemon`].
/// The executable argument is unused on Windows and macOS.
///
/// Copies UTF-8 text or PNG/JPEG/WebP pixels from stdin to the clipboard.
///
/// # Errors
/// Returns an error if input cannot be read or decoded, or clipboard ownership fails.
pub fn copy(daemon_executable: &std::path::Path) -> Result<(), Error> {
    copy_with_format(daemon_executable, Format::Auto)
}

/// Copies stdin using the requested format.
/// # Errors
/// Returns an error if input cannot be read, decoded, or copied.
pub fn copy_with_format(daemon_executable: &std::path::Path, format: Format) -> Result<(), Error> {
    let input = read_stdin()?;

    #[cfg(unix)]
    if osc52::in_ssh_session() {
        require_osc52_text(format)?;
        return osc52::copy(&input);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        daemon::spawn_daemon(daemon_executable, &input, format)
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let _ = daemon_executable;
        let content = parse_content(&input, format)?;
        let mut clipboard = Clipboard::new()
            .map_err(|error| Error::operation("failed to open clipboard", error))?;

        match content {
            Content::Text(text) => clipboard.set_text(text),
            Content::Image(image) => clipboard.set_image(image),
        }
        .map_err(|error| Error::operation("failed to set clipboard", error))
    }
}

/// Runs the long-lived clipboard provider used by `cpy` internally.
///
/// This is intentionally not exposed as a user-facing command. Wayland and X11
/// require the process that owns a selection to keep serving requests, so the
/// public `copy` function starts this function in a detached child process.
///
/// # Errors
/// Returns an error if the provider cannot start, serve, or confirm the selection.
pub fn run_daemon() -> Result<(), Error> {
    run_daemon_with_format(Format::Auto)
}

/// Runs the clipboard provider with the requested input format.
/// # Errors
/// Returns an error if the provider cannot start, serve, or confirm the selection.
pub fn run_daemon_with_format(format: Format) -> Result<(), Error> {
    let input = read_stdin()?;

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        daemon::run(input, format)
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let _ = (input, format);
        Err(Error::Daemon(
            "the clipboard daemon is not supported on Windows or macOS".to_owned(),
        ))
    }
}

fn read_stdin() -> Result<Vec<u8>, Error> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| Error::operation("failed to read stdin", error))?;
    Ok(input)
}

fn parse_content(input: &[u8], format: Format) -> Result<Content, Error> {
    use image::ImageFormat::{Jpeg, Png, WebP};

    let image_format = match format {
        Format::Text => None,
        Format::Png => Some(Png),
        Format::Jpeg => Some(Jpeg),
        Format::Webp => Some(WebP),
        Format::Auto | Format::Image => detect_image_format(input),
    };

    if let Some(image_format) = image_format {
        let operation = match image_format {
            Png => "failed to decode PNG from stdin",
            Jpeg => "failed to decode JPEG from stdin",
            WebP => "failed to decode WebP from stdin",
            _ => "failed to decode image from stdin",
        };
        let image = image::load_from_memory_with_format(input, image_format)
            .map_err(|error| Error::operation(operation, error))?
            .into_rgba8();
        let (width, height) = image.dimensions();
        return Ok(Content::Image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: image.into_raw().into(),
        }));
    }

    if format.requires_image() {
        return Err("stdin is not a PNG, JPEG, or WebP image".into());
    }

    let text = std::str::from_utf8(input).map_err(|error| match format {
        Format::Text => Error::operation("stdin is not UTF-8 text", error),
        _ => Error::InvalidText(error),
    })?;
    Ok(Content::Text(text.to_owned()))
}

fn detect_image_format(input: &[u8]) -> Option<image::ImageFormat> {
    if input.starts_with(PNG_SIGNATURE) {
        Some(image::ImageFormat::Png)
    } else if input.starts_with(JPEG_SIGNATURE) {
        Some(image::ImageFormat::Jpeg)
    } else if input.starts_with(b"RIFF") && input.get(8..12) == Some(b"WEBP") {
        Some(image::ImageFormat::WebP)
    } else {
        None
    }
}

#[cfg(unix)]
fn require_osc52_text(format: Format) -> Result<(), Error> {
    if format.requires_image() {
        return Err("OSC 52 supports only text".into());
    }

    Ok(())
}

/// Writes clipboard text, or a PNG image when stdout is not a terminal.
/// Text takes precedence when both formats are available.
///
/// # Errors
/// Returns an error if the clipboard cannot be read or stdout cannot be written.
pub fn paste() -> Result<(), Error> {
    paste_with_format(Format::Auto)
}

/// Writes clipboard content in the requested format.
/// # Errors
/// Returns an error if the content is unavailable or cannot be written.
pub fn paste_with_format(format: Format) -> Result<(), Error> {
    #[cfg(unix)]
    if osc52::in_ssh_session() {
        require_osc52_text(format)?;
        let text = osc52::paste()?;
        return write_content(&mut io::stdout().lock(), Content::Text(text), format);
    }

    let stdout = io::stdout();
    let is_terminal = stdout.is_terminal();
    if format.requires_image() && is_terminal {
        return Err("redirect stdout to a file or pipe to paste an image".into());
    }

    let mut clipboard =
        Clipboard::new().map_err(|error| Error::operation("failed to open clipboard", error))?;
    let content = if format.requires_image() {
        read_clipboard_image(clipboard.get_image())?
    } else {
        let text_only = is_terminal || matches!(format, Format::Text);
        paste_content(clipboard.get_text(), text_only, || clipboard.get_image())?
    };
    write_content(&mut stdout.lock(), content, format)
}

fn paste_content(
    text: Result<String, arboard::Error>,
    text_only: bool,
    get_image: impl FnOnce() -> Result<ImageData<'static>, arboard::Error>,
) -> Result<Content, Error> {
    match text {
        Ok(text) => Ok(Content::Text(text)),
        Err(arboard::Error::ContentNotAvailable) if !text_only => read_clipboard_image(get_image()),
        Err(error) => Err(Error::operation("failed to read clipboard", error)),
    }
}

fn read_clipboard_image(
    image: Result<ImageData<'static>, arboard::Error>,
) -> Result<Content, Error> {
    image
        .map(Content::Image)
        .map_err(|error| Error::operation("failed to read clipboard image", error))
}

fn write_content(output: &mut impl Write, content: Content, format: Format) -> Result<(), Error> {
    match content {
        Content::Text(text) => output
            .write_all(text.as_bytes())
            .map_err(|error| Error::operation("failed to write stdout", error))?,
        Content::Image(image) => {
            let width = u32::try_from(image.width)
                .map_err(|error| Error::operation("invalid clipboard image width", error))?;
            let height = u32::try_from(image.height)
                .map_err(|error| Error::operation("invalid clipboard image height", error))?;

            match format {
                Format::Jpeg => {
                    let rgba = image::RgbaImage::from_raw(width, height, image.bytes.into_owned())
                        .ok_or_else(|| Error::from("invalid clipboard image dimensions"))?;
                    let rgb = image::DynamicImage::ImageRgba8(rgba).into_rgb8();
                    image::codecs::jpeg::JpegEncoder::new(&mut *output)
                        .write_image(rgb.as_raw(), width, height, ExtendedColorType::Rgb8)
                        .map_err(|error| {
                            Error::operation("failed to write JPEG to stdout", error)
                        })?;
                }
                Format::Webp => image::codecs::webp::WebPEncoder::new_lossless(&mut *output)
                    .write_image(&image.bytes, width, height, ExtendedColorType::Rgba8)
                    .map_err(|error| Error::operation("failed to write WebP to stdout", error))?,
                _ => PngEncoder::new(&mut *output)
                    .write_image(&image.bytes, width, height, ExtendedColorType::Rgba8)
                    .map_err(|error| Error::operation("failed to write PNG to stdout", error))?,
            }
        }
    }

    output
        .flush()
        .map_err(|error| Error::operation("failed to flush stdout", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_selected_image_encoding_and_checks_input_format() {
        let pixels = vec![42, 42, 42, 128];

        for format in [Format::Image, Format::Png, Format::Jpeg, Format::Webp] {
            let image = ImageData {
                width: 1,
                height: 1,
                bytes: pixels.clone().into(),
            };
            let mut output = Vec::new();
            write_content(&mut output, Content::Image(image), format).unwrap();

            let expected_format = match format {
                Format::Jpeg => image::ImageFormat::Jpeg,
                Format::Webp => image::ImageFormat::WebP,
                _ => image::ImageFormat::Png,
            };
            assert_eq!(detect_image_format(&output), Some(expected_format));
            let Content::Image(decoded) = parse_content(&output, format).unwrap() else {
                panic!("expected image");
            };
            assert_eq!((decoded.width, decoded.height), (1, 1));
            assert_eq!(
                decoded.bytes[3],
                if matches!(format, Format::Jpeg) {
                    255
                } else {
                    128
                }
            );
        }
    }

    #[test]
    fn paste_prefers_text() {
        let content = paste_content(Ok("hello 世界".to_owned()), false, || {
            panic!("must not request image when text is available")
        })
        .unwrap();
        let mut output = Vec::new();
        write_content(&mut output, content, Format::Auto).unwrap();
        assert_eq!(output, "hello 世界".as_bytes());
    }

    #[test]
    fn paste_round_trips_image_as_png_without_a_terminal() {
        let pixels = vec![255, 0, 0, 128, 0, 255, 0, 255];
        let content = paste_content(Err(arboard::Error::ContentNotAvailable), false, || {
            Ok(ImageData {
                width: 2,
                height: 1,
                bytes: pixels.clone().into(),
            })
        })
        .unwrap();
        let mut output = Vec::new();
        write_content(&mut output, content, Format::Auto).unwrap();
        let Content::Image(image) = parse_content(&output, Format::Auto).unwrap() else {
            panic!("expected PNG image");
        };
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.bytes.as_ref(), pixels);
    }

    #[test]
    fn paste_does_not_request_images_for_a_terminal_or_clipboard_failure() {
        for (error, is_terminal) in [
            (arboard::Error::ContentNotAvailable, true),
            (arboard::Error::ClipboardOccupied, false),
        ] {
            assert!(
                paste_content(Err(error), is_terminal, || {
                    panic!("must not request image")
                })
                .is_err()
            );
        }
    }

    #[test]
    fn paste_reports_image_read_and_output_failures() {
        let error = paste_content(Err(arboard::Error::ContentNotAvailable), false, || {
            Err(arboard::Error::ContentNotAvailable)
        })
        .err()
        .unwrap();
        assert!(error.to_string().contains("failed to read clipboard image"));

        let error = write_content(
            &mut &mut [0_u8; 0][..],
            Content::Text("hello".to_owned()),
            Format::Auto,
        );
        assert!(error.is_err());
    }

    #[test]
    fn parses_text() {
        let Content::Text(text) = parse_content("hello 世界".as_bytes(), Format::Auto).unwrap()
        else {
            panic!("expected text");
        };
        assert_eq!(text, "hello 世界");
    }

    #[test]
    fn parses_png_as_rgba_image() {
        let pixels = [255, 0, 0, 128];
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&pixels, 1, 1, ExtendedColorType::Rgba8)
            .unwrap();

        let Content::Image(image) = parse_content(&png, Format::Auto).unwrap() else {
            panic!("expected image");
        };
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.bytes.as_ref(), pixels);
    }

    #[test]
    fn parses_jpeg_as_rgba_and_pastes_as_png() {
        for color in [ExtendedColorType::Rgb8, ExtendedColorType::L8] {
            let pixels = vec![42; color.bits_per_pixel() as usize / 8 * 2];
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
                .write_image(&pixels, 2, 1, color)
                .unwrap();

            let Content::Image(image) = parse_content(&jpeg, Format::Auto).unwrap() else {
                panic!("expected JPEG image");
            };
            assert_eq!((image.width, image.height), (2, 1));
            assert_eq!(image.bytes.len(), 8);
            for pixel in image.bytes.as_chunks::<4>().0 {
                assert!(pixel[..3].iter().all(|&value| value.abs_diff(42) <= 1));
                assert_eq!(pixel[3], 255);
            }

            let expected = image.bytes.to_vec();
            let mut output = Vec::new();
            write_content(&mut output, Content::Image(image), Format::Auto).unwrap();
            assert!(output.starts_with(PNG_SIGNATURE));
            let Content::Image(image) = parse_content(&output, Format::Auto).unwrap() else {
                panic!("expected PNG image");
            };
            assert_eq!((image.width, image.height), (2, 1));
            assert_eq!(image.bytes.as_ref(), expected);
        }
    }

    #[test]
    fn parses_webp_as_rgba() {
        let pixels = [255, 0, 0, 255, 0, 0, 255, 128];
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .write_image(&pixels, 2, 1, ExtendedColorType::Rgba8)
            .unwrap();

        let Content::Image(image) = parse_content(&webp, Format::Auto).unwrap() else {
            panic!("expected WebP image");
        };
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.bytes.as_ref(), pixels);
    }

    #[test]
    fn reports_invalid_input() {
        use std::error::Error as _;

        let png_error = parse_content(b"\x89PNG\r\n\x1a\ninvalid", Format::Auto)
            .err()
            .unwrap();
        assert!(png_error.to_string().contains("decode PNG"));
        assert!(png_error.source().unwrap().is::<image::ImageError>());

        let jpeg_error = parse_content(b"\xff\xd8\xffinvalid", Format::Auto)
            .err()
            .unwrap();
        assert!(jpeg_error.to_string().contains("decode JPEG"));
        assert!(jpeg_error.source().unwrap().is::<image::ImageError>());

        let webp_error = parse_content(b"RIFF\x04\0\0\0WEBP", Format::Auto)
            .err()
            .unwrap();
        assert!(webp_error.to_string().contains("decode WebP"));
        assert!(webp_error.source().unwrap().is::<image::ImageError>());

        let text_error = parse_content(b"\xff", Format::Auto).err().unwrap();
        assert!(matches!(text_error, Error::InvalidText(_)));
        assert!(text_error.to_string().contains("neither UTF-8"));
        assert!(text_error.source().unwrap().is::<std::str::Utf8Error>());
    }

    #[test]
    fn preserves_text_boundaries() {
        for text in ["", "hello 世界\n\n"] {
            let mut output = Vec::new();
            write_content(
                &mut output,
                parse_content(text.as_bytes(), Format::Auto).unwrap(),
                Format::Auto,
            )
            .unwrap();
            assert_eq!(output, text.as_bytes());
        }
    }

    #[test]
    fn converts_rgb_and_grayscale_to_rgba() {
        for (color, pixels, expected) in [
            (ExtendedColorType::Rgb8, vec![10, 20, 30], [10, 20, 30, 255]),
            (ExtendedColorType::L8, vec![42], [42, 42, 42, 255]),
        ] {
            let mut png = Vec::new();
            PngEncoder::new(&mut png)
                .write_image(&pixels, 1, 1, color)
                .unwrap();
            let Content::Image(image) = parse_content(&png, Format::Auto).unwrap() else {
                panic!("expected image")
            };
            assert_eq!((image.width, image.height), (1, 1));
            assert_eq!(image.bytes.as_ref(), expected);
        }
    }

    #[test]
    fn reports_png_write_and_flush_failures() {
        use std::error::Error as _;
        struct Writer(bool);

        impl Write for Writer {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.0 {
                    Err(io::Error::other("write failed"))
                } else {
                    Ok(bytes.len())
                }
            }

            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("flush failed"))
            }
        }

        for fail_write in [true, false] {
            let image = ImageData {
                width: 1,
                height: 1,
                bytes: vec![0, 0, 0, 255].into(),
            };
            let error = write_content(&mut Writer(fail_write), Content::Image(image), Format::Auto)
                .unwrap_err();
            let operation = if fail_write {
                "failed to write PNG to stdout"
            } else {
                "failed to flush stdout"
            };
            assert!(error.to_string().contains(operation), "{error}");
            if fail_write {
                assert!(error.source().unwrap().is::<image::ImageError>());
            } else {
                assert!(error.source().unwrap().is::<io::Error>());
            }
        }
    }
}
