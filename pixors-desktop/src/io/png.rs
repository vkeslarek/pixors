//! PNG image loading and saving.

use crate::color::{ColorSpace, TransferFn, RgbPrimaries};
use crate::error::Error;
use crate::image::buffer::BufferDesc;
use crate::image::{AlphaMode, ImageBuffer, Layer, LayerMetadata, ImageInfo, Orientation, ImageMetadata, TileCoord};
use crate::io::accumulator::RowAccumulator;
use crate::io::ImageReader;
use crate::storage::writer::TileWriter;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use png::{Decoder, Encoder, ColorType, BitDepth, Transformations};

/// PNG format reader.
pub struct PngFormat;

impl ImageReader for PngFormat {
    fn can_handle(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            .unwrap_or(false)
    }

    fn read_document_info(&self, path: &Path) -> Result<ImageInfo, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = Decoder::new(reader);
        decoder.set_transformations(Transformations::EXPAND);
        let reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
        let info = reader.info();
        let metadata = Self::document_metadata_from_info(info, path);
        Ok(ImageInfo { layer_count: 1, metadata })
    }

    fn read_layer_metadata(&self, path: &Path, layer: usize) -> Result<LayerMetadata, Error> {
        if layer != 0 {
            return Err(Error::invalid_param(format!("PNG has only 1 layer, requested {}", layer)));
        }
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = Decoder::new(reader);
        decoder.set_transformations(Transformations::EXPAND);
        let reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
        let info = reader.info();
        let (w, h) = (info.width, info.height);
        let cs = Self::detect_color_space(info);
        let am = AlphaMode::Straight;
        let is_16bit = matches!(info.bit_depth, BitDepth::Sixteen);
        let desc = Self::png_buffer_desc(info, w, h, cs, am, is_16bit);
        Ok(LayerMetadata {
            desc,
            orientation: Orientation::Identity,
            offset: (0, 0),
            name: path.file_stem().and_then(|s| s.to_str()).unwrap_or("PNG").to_string(),
        })
    }

    fn load_layer(&self, path: &Path, layer: usize) -> Result<Layer, Error> {
        if layer != 0 {
            return Err(Error::invalid_param(format!("PNG has only 1 layer, requested {}", layer)));
        }
        let buf = Self::load_png(path)?;
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("PNG").to_string();
        Ok(Layer::from_buffer(name, buf))
    }

    fn stream_tiles(
        &self,
        path: &Path,
        tile_size: u32,
        writer: &dyn TileWriter<u8>,
        layer: usize,
        on_progress: Option<&(dyn Fn(u8) + Send)>,
    ) -> Result<(), Error> {
        if layer != 0 {
            return Err(Error::invalid_param("PNG has only 1 layer"));
        }
        let file = File::open(path).map_err(Error::Io)?;
        let mut decoder = Decoder::new(BufReader::new(file));
        decoder.set_transformations(Transformations::EXPAND);
        let mut reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
        let info = reader.info();
        let (w, h) = (info.width, info.height);

        // Interlaced images: fall back to full decode + band slice
        if info.interlaced {
            let buf = Self::load_png(path)?;
            return Self::stream_tiles_from_buffer(&buf, tile_size, writer, on_progress);
        }

        let cs = Self::detect_color_space(info);
        let am = AlphaMode::Straight;
        let is_16bit = matches!(info.bit_depth, BitDepth::Sixteen);
        let desc = Self::png_buffer_desc(info, w, h, cs, am, is_16bit);

        let mut acc = RowAccumulator::new(w, h, tile_size, desc, 64 * 1024 * 1024);
        let tiles_y = h.div_ceil(tile_size);

        while let Some(row) = reader.next_row().map_err(|e| Error::Png(e.to_string()))? {
            acc.push_row(row.data());
            if acc.is_full() {
                for frag in acc.extract_tiles() {
                    writer.write_tile(frag.coord, &frag.data)?;
                }
                let band_ty = acc.band_ty();
                if let Some(cb) = on_progress {
                    cb(((band_ty + 1) * 100 / tiles_y) as u8);
                }
                acc.reset();
            }
        }
        // Flush remaining partial band
        if acc.rows_filled() > 0 {
            for frag in acc.extract_tiles() {
                writer.write_tile(frag.coord, &frag.data)?;
            }
        }
        writer.finish()?;
        Ok(())
    }
}

impl PngFormat {
    fn stream_tiles_from_buffer(
        buf: &ImageBuffer,
        tile_size: u32,
        writer: &dyn TileWriter<u8>,
        on_progress: Option<&(dyn Fn(u8) + Send)>,
    ) -> Result<(), Error> {
        let w = buf.desc.width;
        let h = buf.desc.height;
        let bpp = buf.desc.planes.len() * buf.desc.planes[0].encoding.byte_size();
        let tiles_y = h.div_ceil(tile_size);
        for band_ty in 0..tiles_y {
            let band_start_y = band_ty * tile_size;
            let band_height = (h - band_start_y).min(tile_size);
            let tiles_x = w.div_ceil(tile_size);
            let row_stride = w as usize * bpp;
            for tx in 0..tiles_x {
                let tile_px = tx * tile_size;
                let actual_w = (w - tile_px).min(tile_size);
                let coord = TileCoord::new(0, tx, band_ty, tile_size, w, h);
                let mut tile_data = vec![0u8; (actual_w * band_height) as usize * bpp];
                let tile_stride = actual_w as usize * bpp;
                for r in 0..band_height as usize {
                    let src_start = ((band_start_y as usize + r) * row_stride) + tile_px as usize * bpp;
                    let dst_start = r * tile_stride;
                    tile_data[dst_start..dst_start + tile_stride]
                        .copy_from_slice(&buf.data[src_start..src_start + tile_stride]);
                }
                writer.write_tile(coord, &tile_data)?;
            }
            if let Some(cb) = on_progress {
                cb(((band_ty + 1) * 100 / tiles_y) as u8);
            }
        }
        writer.finish()?;
        Ok(())
    }

    /// Extract document-level metadata from a PNG's Info struct.
    fn document_metadata_from_info(info: &png::Info, path: &Path) -> ImageMetadata {
        use std::collections::HashMap;

        let source_format = Some("PNG".to_string());
        let source_path = Some(path.to_path_buf());
        let mut text = HashMap::new();
        let mut raw_icc = None;
        let mut dpi = None;

        // Textual metadata
        for t in &info.uncompressed_latin1_text {
            text.insert(t.keyword.clone(), t.text.clone());
        }
        for t in &info.compressed_latin1_text {
            text.insert(t.keyword.clone(), t.get_text().unwrap_or_default());
        }
        for t in &info.utf8_text {
            text.insert(t.keyword.clone(), t.get_text().unwrap_or_default());
        }

        // ICC profile bytes
        if let Some(icc) = &info.icc_profile {
            raw_icc = Some(icc.to_vec());
        }

        // Physical dimensions (pHYs)
        if let Some(pdim) = info.pixel_dims
            && pdim.unit == png::Unit::Meter
        {
            let dpi_x = pdim.xppu as f32 * 0.0254;
            let dpi_y = pdim.yppu as f32 * 0.0254;
            dpi = Some((dpi_x, dpi_y));
        }

        ImageMetadata { source_format, source_path, dpi, text, raw_icc }
    }

    /// Build the correct BufferDesc for PNG color type + bit depth.
    fn png_buffer_desc(
        info: &png::Info,
        w: u32, h: u32,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
        is_16bit: bool,
    ) -> BufferDesc {
        match (info.color_type, is_16bit) {
            (png::ColorType::Grayscale, false) => BufferDesc::gray8_interleaved(w, h, color_space, alpha_mode),
            (png::ColorType::Grayscale, true) => BufferDesc::gray16be_interleaved(w, h, color_space, alpha_mode),
            (png::ColorType::GrayscaleAlpha, false) => BufferDesc::gray_alpha8_interleaved(w, h, color_space, alpha_mode),
            (png::ColorType::GrayscaleAlpha, true) => BufferDesc::gray_alpha16be_interleaved(w, h, color_space, alpha_mode),
            (png::ColorType::Rgb, false) => BufferDesc::rgb8_interleaved(w, h, color_space, alpha_mode),
            (png::ColorType::Rgb, true) => BufferDesc::rgb16be_interleaved(w, h, color_space, alpha_mode),
            (png::ColorType::Rgba, false) => BufferDesc::rgba8_interleaved(w, h, color_space, alpha_mode),
            (png::ColorType::Rgba, true) => BufferDesc::rgba16be_interleaved(w, h, color_space, alpha_mode),
            (png::ColorType::Indexed, _) => {
                if info.trns.is_some() {
                    BufferDesc::rgba8_interleaved(w, h, color_space, alpha_mode)
                } else {
                    BufferDesc::rgb8_interleaved(w, h, color_space, alpha_mode)
                }
            }
        }
    }

    /// Loads a PNG image from a file path.
    fn load_png(path: &Path) -> Result<ImageBuffer, Error> {
        let _sw = crate::debug_stopwatch!("load_png");
        tracing::info!("Reading PNG from {:?}", path);

        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);

        let mut decoder = Decoder::new(reader);
        decoder.set_transformations(Transformations::EXPAND);

        let mut reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;

        let info = reader.info();
        let width = info.width;
        let height = info.height;
        let bit_depth = info.bit_depth;

        let color_space = Self::detect_color_space(info);
        let alpha_mode = AlphaMode::Straight;
        let is_16bit = matches!(bit_depth, BitDepth::Sixteen);

        let desc = Self::png_buffer_desc(info, width, height, color_space, alpha_mode, is_16bit);

        let buf_size = reader.output_buffer_size().expect("PNG decoder output buffer size unavailable");
        let mut buf = vec![0; buf_size];
        let info = reader.next_frame(&mut buf).map_err(|e| Error::Png(e.to_string()))?;
        let data = buf[..info.buffer_size()].to_vec();

        Ok(ImageBuffer { desc, data })
    }

    /// Detects the color space from PNG metadata.
    fn detect_color_space(info: &png::Info) -> ColorSpace {
        use crate::color::detect;

        // Priority 1: cICP chunk (new, explicit)
        if let Some(cicp) = info.coding_independent_code_points {
            let primaries = match cicp.color_primaries {
                1  => Some(RgbPrimaries::Bt709),
                9  => Some(RgbPrimaries::Bt2020),
                11 => Some(RgbPrimaries::P3),
                _  => None,
            };
            let transfer = match cicp.transfer_function {
                1  => Some(TransferFn::Rec709Gamma),
                13 => Some(TransferFn::SrgbGamma),
                14 => Some(TransferFn::Gamma22),
                15 => Some(TransferFn::Gamma24),
                16 => Some(TransferFn::ProPhotoGamma),
                _  => None,
            };
            if primaries.is_some() && transfer.is_some() {
                return ColorSpace::with_optional_params(primaries, None, transfer);
            }
        }

        // Priority 2: iCCP chunk
        if let Some(icc_bytes) = &info.icc_profile {
            let classified = detect::IccClassification::classify_icc_profile(icc_bytes);
            if let Some(cs) = classified.color_space {
                return cs;
            }
            tracing::warn!("Unrecognized ICC profile (desc: {}), assuming sRGB",
                String::from_utf8_lossy(&classified.raw).chars().take(60).collect::<String>());
            return ColorSpace::SRGB;
        }

        // Priority 3: sRGB chunk
        if info.srgb.is_some() {
            return ColorSpace::SRGB;
        }

        // Priority 4: gAMA + cHRM chunks (use shared chromaticity matcher)
        let mut gamma = None;
        if let Some(g) = info.gamma() {
            gamma = Some(g.into_value());
        }
        if let Some(chrm) = info.chromaticities() {
            if let Some((prim, wp)) = detect::match_chromaticities(
                chrm.white.0.into_value(), chrm.white.1.into_value(),
                chrm.red.0.into_value(),   chrm.red.1.into_value(),
                chrm.green.0.into_value(), chrm.green.1.into_value(),
                chrm.blue.0.into_value(),  chrm.blue.1.into_value(),
                0.002,
            ) {
                let transfer = gamma
                    .and_then(TransferFn::from_gamma)
                    .unwrap_or(TransferFn::SrgbGamma);
                return ColorSpace::new(prim, wp, transfer);
            }
            if let Some(g) = gamma
                && let Some(tf) = TransferFn::from_gamma(g)
            {
                return ColorSpace::with_optional_params(None, None, Some(tf));
            }
        }

        // Priority 5: gAMA alone
        if let Some(g) = gamma
            && let Some(tf) = TransferFn::from_gamma(g)
        {
            return ColorSpace::with_optional_params(None, None, Some(tf));
        }

        // No color info → assume sRGB
        tracing::warn!("No color space metadata in PNG, assuming sRGB");
        ColorSpace::SRGB
    }
}

/// Saves a raw image as PNG to a file path.
pub fn save_png(raw: &ImageBuffer, path: &std::path::Path) -> Result<(), Error> {
    let num_planes = raw.desc.planes.len();

    let (color_type, bit_depth) = match num_planes {
        1 => (ColorType::Grayscale, BitDepth::Eight),
        2 => (ColorType::GrayscaleAlpha, BitDepth::Eight),
        3 => (ColorType::Rgb, BitDepth::Eight),
        4 => (ColorType::Rgba, BitDepth::Eight),
        _ => return Err(Error::unsupported_sample_type(
            format!("Unsupported number of planes for PNG: {}", num_planes)
        )),
    };

    let file = File::create(path).map_err(Error::Io)?;
    let w = std::io::BufWriter::new(file);

    let mut encoder = Encoder::new(w, raw.desc.width, raw.desc.height);
    encoder.set_color(color_type);
    encoder.set_depth(bit_depth);

    match raw.desc.color_space {
        ColorSpace::SRGB => {
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        }
        _ => {
            tracing::warn!(
                "PNG save for {:?} missing metadata; only sRGB is fully supported",
                raw.desc.color_space
            );
        }
    }

    let mut writer = encoder.write_header().map_err(|e| Error::Png(e.to_string()))?;
    writer.write_image_data(&raw.data).map_err(|e| Error::Png(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore]
    fn roundtrip_save_load() {
        // TODO: Update test to use new ImageBuffer API
    }
}
