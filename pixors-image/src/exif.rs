//! Unified cross-format EXIF/metadata — populated by PNG / TIFF / JPEG codecs.
//!
//! Each variant maps to one metadata field. Codecs extract format-specific
//! sources (EXIF tags, TIFF tags, PNG chunks) into this common representation.
//! Fields that don't fit a named variant go into `Custom { key, value }`.

use serde::{Deserialize, Serialize};

/// Typed image metadata, format-agnostic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Metadata {
    // ── Identity ──────────────────────────────────────────────────────────
    Make(String),
    Model(String),
    Software(String),
    HostComputer(String),

    // ── Author / Rights ──────────────────────────────────────────────────
    Artist(String),
    Copyright(String),
    Description(String),
    DocumentName(String),

    // ── Date / Time ──────────────────────────────────────────────────────
    DateTime(String),
    DateTimeOriginal(String),
    DateTimeDigitized(String),
    SubSecTimeOriginal(String),

    // ── Camera ───────────────────────────────────────────────────────────
    ExposureTime(String),
    FNumber(String),
    ExposureProgram(u16),
    ISOSpeedRatings(u32),
    FocalLength(String),
    FocalLengthIn35mm(u16),
    Flash(u16),
    ExposureBias(String),
    MeteringMode(u16),
    LensModel(String),
    LensMake(String),
    MaxAperture(String),
    WhiteBalance(u16),
    LightSource(u16),
    SceneCaptureType(u16),
    Contrast(u16),
    Saturation(u16),
    Sharpness(u16),

    // ── Image ────────────────────────────────────────────────────────────
    ImageWidth(u32),
    ImageHeight(u32),
    BitsPerSample(Vec<u16>),
    Dpi {
        x: f32,
        y: f32,
    },
    Orientation(u16),
    ExifVersion(String),
    ColorSpaceTag(String),

    // ── Color / HDR ─────────────────────────────────────────────────────
    WhitePoint([f64; 2]),
    PrimaryChromaticities {
        red: [f64; 2],
        green: [f64; 2],
        blue: [f64; 2],
    },
    Gamma(f64),
    IccProfile(Vec<u8>),
    PhotometricInterpretation(u16),

    // ── HDR (PNG) ───────────────────────────────────────────────────────
    MasteringDisplayLuminance {
        min: f64,
        max: f64,
    },
    ContentLightLevel {
        max_fall: f64,
        max_cll: f64,
    },

    // ── GPS ─────────────────────────────────────────────────────────────
    GpsLatitudeRef(String),
    GpsLatitude(Vec<f64>),
    GpsLongitudeRef(String),
    GpsLongitude(Vec<f64>),
    GpsAltitudeRef(u8),
    GpsAltitude(f64),
    GpsDateStamp(String),

    // ── Compression / Layout ────────────────────────────────────────────
    Compression(u16),
    PlanarConfiguration(u16),

    // ── Catch-all ────────────────────────────────────────────────────────
    Custom {
        key: String,
        value: String,
    },
}

// ── EXIF → Metadata mapper ─────────────────────────────────────────────────

fn exif_ascii(f: &exif::Field) -> Option<String> {
    match &f.value {
        exif::Value::Ascii(v) if !v.is_empty() => {
            let s = String::from_utf8_lossy(&v[0]);
            let trimmed = s.trim_end_matches('\0');
            Some(trimmed.to_string())
        }
        _ => None,
    }
}

fn exif_uint(f: &exif::Field) -> Option<u32> {
    f.value.get_uint(0)
}

fn exif_rational(f: &exif::Field) -> Option<String> {
    match &f.value {
        exif::Value::Rational(v) if !v.is_empty() => Some(format!("{}/{}", v[0].num, v[0].denom)),
        exif::Value::SRational(v) if !v.is_empty() => {
            let n = v[0].num;
            let d = v[0].denom;
            if d.is_negative() {
                Some(format!("{}/{}", n as i64, d as i64))
            } else {
                Some(format!("{}/{}", n, d))
            }
        }
        _ => None,
    }
}

/// Map EXIF `Rational` to a float string or rational string.
fn exif_rational_float(f: &exif::Field) -> Option<String> {
    match &f.value {
        exif::Value::Rational(v) if !v.is_empty() => {
            if v[0].denom == 0 {
                None
            } else {
                let val = v[0].num as f64 / v[0].denom as f64;
                Some(format!("{:.6}", val))
            }
        }
        _ => None,
    }
}

/// Map a batch of EXIF fields into `Vec<Metadata>`.
pub fn from_exif_fields(fields: &[exif::Field]) -> Vec<Metadata> {
    let mut out = Vec::new();

    for f in fields {
        let m = match f.tag {
            // Identity
            exif::Tag::Make => exif_ascii(f).map(Metadata::Make),
            exif::Tag::Model => exif_ascii(f).map(Metadata::Model),
            exif::Tag::Software => exif_ascii(f).map(Metadata::Software),

            // Author
            exif::Tag::Artist => exif_ascii(f).map(Metadata::Artist),
            exif::Tag::Copyright => exif_ascii(f).map(Metadata::Copyright),
            exif::Tag::ImageDescription => exif_ascii(f).map(Metadata::Description),

            // Date
            exif::Tag::DateTime => exif_ascii(f).map(Metadata::DateTime),
            exif::Tag::DateTimeOriginal => exif_ascii(f).map(Metadata::DateTimeOriginal),
            exif::Tag::DateTimeDigitized => exif_ascii(f).map(Metadata::DateTimeDigitized),
            exif::Tag::SubSecTimeOriginal => exif_ascii(f).map(Metadata::SubSecTimeOriginal),

            // Camera
            exif::Tag::ExposureTime => exif_rational(f).map(Metadata::ExposureTime),
            exif::Tag::FNumber => exif_rational_float(f).map(Metadata::FNumber),
            exif::Tag::ExposureProgram => exif_uint(f).map(|v| Metadata::ExposureProgram(v as u16)),
            exif::Tag::PhotographicSensitivity => exif_uint(f).map(Metadata::ISOSpeedRatings),
            exif::Tag::FocalLength => exif_rational_float(f).map(Metadata::FocalLength),
            exif::Tag::FocalLengthIn35mmFilm => {
                exif_uint(f).map(|v| Metadata::FocalLengthIn35mm(v as u16))
            }
            exif::Tag::Flash => exif_uint(f).map(|v| Metadata::Flash(v as u16)),
            exif::Tag::ExposureBiasValue => exif_rational(f).map(Metadata::ExposureBias),
            exif::Tag::MeteringMode => exif_uint(f).map(|v| Metadata::MeteringMode(v as u16)),
            exif::Tag::LensModel => exif_ascii(f).map(Metadata::LensModel),
            exif::Tag::LensMake => exif_ascii(f).map(Metadata::LensMake),
            exif::Tag::MaxApertureValue => exif_rational_float(f).map(Metadata::MaxAperture),
            exif::Tag::WhiteBalance => exif_uint(f).map(|v| Metadata::WhiteBalance(v as u16)),
            exif::Tag::LightSource => exif_uint(f).map(|v| Metadata::LightSource(v as u16)),
            exif::Tag::SceneCaptureType => {
                exif_uint(f).map(|v| Metadata::SceneCaptureType(v as u16))
            }
            exif::Tag::Contrast => exif_uint(f).map(|v| Metadata::Contrast(v as u16)),
            exif::Tag::Saturation => exif_uint(f).map(|v| Metadata::Saturation(v as u16)),
            exif::Tag::Sharpness => exif_uint(f).map(|v| Metadata::Sharpness(v as u16)),

            // Image
            exif::Tag::ImageWidth => exif_uint(f).map(Metadata::ImageWidth),
            exif::Tag::ImageLength => exif_uint(f).map(Metadata::ImageHeight),
            exif::Tag::Orientation => exif_uint(f).map(|v| Metadata::Orientation(v as u16)),
            exif::Tag::ExifVersion => exif_ascii(f).map(Metadata::ExifVersion),

            // GPS
            exif::Tag::GPSLatitudeRef => exif_ascii(f).map(Metadata::GpsLatitudeRef),
            exif::Tag::GPSLatitude => {
                if let exif::Value::Rational(v) = &f.value {
                    Some(Metadata::GpsLatitude(
                        v.iter().map(|r| r.num as f64 / r.denom as f64).collect(),
                    ))
                } else {
                    None
                }
            }
            exif::Tag::GPSLongitudeRef => exif_ascii(f).map(Metadata::GpsLongitudeRef),
            exif::Tag::GPSLongitude => {
                if let exif::Value::Rational(v) = &f.value {
                    Some(Metadata::GpsLongitude(
                        v.iter().map(|r| r.num as f64 / r.denom as f64).collect(),
                    ))
                } else {
                    None
                }
            }
            exif::Tag::GPSAltitudeRef => exif_uint(f).map(|v| Metadata::GpsAltitudeRef(v as u8)),
            exif::Tag::GPSAltitude => exif_rational_float(f)
                .and_then(|s| s.parse().ok())
                .map(Metadata::GpsAltitude),
            exif::Tag::GPSDateStamp => exif_ascii(f).map(Metadata::GpsDateStamp),

            // BitsPerSample
            exif::Tag::BitsPerSample => {
                if let exif::Value::Short(v) = &f.value {
                    Some(Metadata::BitsPerSample(v.clone()))
                } else {
                    None
                }
            }

            // Unknown — skip, too noisy
            _ => None,
        };

        if let Some(m) = m {
            out.push(m);
        }
    }

    out
}

/// Map PNG text chunks (tEXt / iTXt keyword-value pairs) to Metadata.
pub fn from_png_text(texts: &[(&str, String)]) -> Vec<Metadata> {
    let mut out = Vec::new();
    for (key, value) in texts {
        match key.to_lowercase().as_str() {
            "author" | "artist" => out.push(Metadata::Artist(value.clone())),
            "description" | "title" => out.push(Metadata::Description(value.clone())),
            "copyright" => out.push(Metadata::Copyright(value.clone())),
            "software" => out.push(Metadata::Software(value.clone())),
            _ => out.push(Metadata::Custom {
                key: key.to_string(),
                value: value.clone(),
            }),
        }
    }
    out
}

// ── Helpers ──────────────────────────────────────────────────────────────────

impl Metadata {
    /// Human-readable label for UI display.
    pub fn label(&self) -> &str {
        match self {
            Metadata::Make(_) => "Make",
            Metadata::Model(_) => "Model",
            Metadata::Software(_) => "Software",
            Metadata::HostComputer(_) => "Host Computer",
            Metadata::Artist(_) => "Artist",
            Metadata::Copyright(_) => "Copyright",
            Metadata::Description(_) => "Description",
            Metadata::DocumentName(_) => "Document Name",
            Metadata::DateTime(_) => "Date/Time",
            Metadata::DateTimeOriginal(_) => "Original Date",
            Metadata::DateTimeDigitized(_) => "Digitized Date",
            Metadata::SubSecTimeOriginal(_) => "Sub-second Time",
            Metadata::ExposureTime(_) => "Exposure",
            Metadata::FNumber(_) => "Aperture",
            Metadata::ExposureProgram(_) => "Exposure Program",
            Metadata::ISOSpeedRatings(_) => "ISO",
            Metadata::FocalLength(_) => "Focal Length",
            Metadata::FocalLengthIn35mm(_) => "Focal (35mm eq.)",
            Metadata::Flash(_) => "Flash",
            Metadata::ExposureBias(_) => "Exposure Bias",
            Metadata::MeteringMode(_) => "Metering",
            Metadata::LensModel(_) => "Lens",
            Metadata::LensMake(_) => "Lens Make",
            Metadata::MaxAperture(_) => "Max Aperture",
            Metadata::WhiteBalance(_) => "White Balance",
            Metadata::LightSource(_) => "Light Source",
            Metadata::SceneCaptureType(_) => "Scene Type",
            Metadata::Contrast(_) => "Contrast",
            Metadata::Saturation(_) => "Saturation",
            Metadata::Sharpness(_) => "Sharpness",
            Metadata::ImageWidth(_) => "Width",
            Metadata::ImageHeight(_) => "Height",
            Metadata::BitsPerSample(_) => "Bits/Sample",
            Metadata::Dpi { .. } => "DPI",
            Metadata::Orientation(_) => "Orientation",
            Metadata::ExifVersion(_) => "EXIF Version",
            Metadata::ColorSpaceTag(_) => "Color Space",
            Metadata::WhitePoint(_) => "White Point",
            Metadata::PrimaryChromaticities { .. } => "Primaries",
            Metadata::Gamma(_) => "Gamma",
            Metadata::IccProfile(_) => "ICC Profile",
            Metadata::PhotometricInterpretation(_) => "Photometric",
            Metadata::MasteringDisplayLuminance { .. } => "HDR Display Luminance",
            Metadata::ContentLightLevel { .. } => "HDR Light Level",
            Metadata::GpsLatitudeRef(_) => "GPS Lat Ref",
            Metadata::GpsLatitude(_) => "GPS Latitude",
            Metadata::GpsLongitudeRef(_) => "GPS Long Ref",
            Metadata::GpsLongitude(_) => "GPS Longitude",
            Metadata::GpsAltitudeRef(_) => "GPS Alt. Ref",
            Metadata::GpsAltitude(_) => "GPS Altitude",
            Metadata::GpsDateStamp(_) => "GPS Date",
            Metadata::Compression(_) => "Compression",
            Metadata::PlanarConfiguration(_) => "Planar Config",
            Metadata::Custom { key, .. } => key,
        }
    }

    /// Human-readable value for UI display.
    pub fn value_str(&self) -> String {
        match self {
            Metadata::Make(v)
            | Metadata::Model(v)
            | Metadata::Software(v)
            | Metadata::HostComputer(v)
            | Metadata::Artist(v)
            | Metadata::Copyright(v)
            | Metadata::Description(v)
            | Metadata::DocumentName(v)
            | Metadata::DateTime(v)
            | Metadata::DateTimeOriginal(v)
            | Metadata::DateTimeDigitized(v)
            | Metadata::SubSecTimeOriginal(v)
            | Metadata::ExposureTime(v)
            | Metadata::FNumber(v)
            | Metadata::FocalLength(v)
            | Metadata::ExposureBias(v)
            | Metadata::LensModel(v)
            | Metadata::LensMake(v)
            | Metadata::MaxAperture(v)
            | Metadata::ExifVersion(v)
            | Metadata::ColorSpaceTag(v)
            | Metadata::GpsLatitudeRef(v)
            | Metadata::GpsLongitudeRef(v)
            | Metadata::GpsDateStamp(v) => v.clone(),
            Metadata::ExposureProgram(v)
            | Metadata::FocalLengthIn35mm(v)
            | Metadata::Flash(v)
            | Metadata::MeteringMode(v)
            | Metadata::WhiteBalance(v)
            | Metadata::LightSource(v)
            | Metadata::SceneCaptureType(v)
            | Metadata::Contrast(v)
            | Metadata::Saturation(v)
            | Metadata::Sharpness(v)
            | Metadata::Orientation(v)
            | Metadata::PhotometricInterpretation(v)
            | Metadata::Compression(v)
            | Metadata::PlanarConfiguration(v) => format!("{v}"),
            Metadata::GpsAltitudeRef(v) => format!("{v}"),
            Metadata::ISOSpeedRatings(v) | Metadata::ImageWidth(v) | Metadata::ImageHeight(v) => {
                format!("{v}")
            }
            Metadata::BitsPerSample(v) => format!("{:?}", v),
            Metadata::Dpi { x, y } => format!("{:.0}×{:.0}", x, y),
            Metadata::WhitePoint(v) => format!("({:.4}, {:.4})", v[0], v[1]),
            Metadata::PrimaryChromaticities { red, green, blue } => {
                format!(
                    "R({:.4},{:.4}) G({:.4},{:.4}) B({:.4},{:.4})",
                    red[0], red[1], green[0], green[1], blue[0], blue[1]
                )
            }
            Metadata::Gamma(v) => format!("{:.4}", v),
            Metadata::IccProfile(v) => format!("{} bytes", v.len()),
            Metadata::MasteringDisplayLuminance { min, max } => {
                format!("min={:.2} max={:.2} cd/m²", min, max)
            }
            Metadata::ContentLightLevel { max_fall, max_cll } => {
                format!("MaxFALL={:.2} MaxCLL={:.2}", max_fall, max_cll)
            }
            Metadata::GpsLatitude(v) | Metadata::GpsLongitude(v) => {
                format!("{:?}", v)
            }
            Metadata::GpsAltitude(v) => format!("{:.1}m", v),
            Metadata::Custom { key: _, value } => value.clone(),
        }
    }
}
