use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::model::color::space::ColorSpace;
use crate::model::image::AlphaMode;
use crate::model::image::buffer::{BufferDesc, PlaneDesc, SampleFormat};
use crate::model::image::document::{LayerMetadata, Orientation, BlendMode};
use crate::source::image_file_source::ImageFileSource;

#[derive(Debug, Clone)]
pub struct ImageFile {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub color_space: ColorSpace,
    pub layer_infos: Vec<LayerFileInfo>,
}

#[derive(Debug, Clone)]
pub struct LayerFileInfo {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub orientation: Orientation,
    pub offset: (i32, i32),
    pub blend_mode: BlendMode,
    pub desc: BufferDesc,
}

impl ImageFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext.to_lowercase().as_str() {
            "png" => Self::open_png(&path),
            "tiff" | "tif" => Self::open_tiff(&path),
            other => Err(Error::internal(format!("unsupported format: {other}"))),
        }
    }

    pub fn layers(&self) -> &[LayerFileInfo] {
        &self.layer_infos
    }

    pub fn source(&self, layer_index: usize) -> ImageFileSource {
        let li = &self.layer_infos[layer_index];
        ImageFileSource {
            path: self.path.clone(),
            layer_index,
            layer_name: li.name.clone(),
        }
    }

    fn open_png(path: &Path) -> Result<Self, Error> {
        use std::fs::File;
        use std::io::BufReader;
        let file = File::open(path)?;
        let decoder = png::Decoder::new(BufReader::new(file));
        let reader = decoder
            .read_info()
            .map_err(|e| Error::Png(e.to_string()))?;
        let info = reader.info();

        let desc = BufferDesc {
            width: info.width,
            height: info.height,
            planes: vec![PlaneDesc {
                offset: 0,
                stride: 4,
                row_stride: info.width as usize * 4,
                row_length: info.width,
                encoding: SampleFormat::U8,
            }],
            color_space: ColorSpace::SRGB,
            alpha_mode: AlphaMode::Premultiplied,
        };

        Ok(Self {
            path: path.to_path_buf(),
            width: info.width,
            height: info.height,
            color_space: ColorSpace::SRGB,
            layer_infos: vec![LayerFileInfo {
                name: path.file_stem().unwrap_or_default().to_string_lossy().into(),
                visible: true,
                opacity: 1.0,
                orientation: Orientation::Identity,
                offset: (0, 0),
                blend_mode: BlendMode::Normal,
                desc,
            }],
        })
    }

    fn open_tiff(path: &Path) -> Result<Self, Error> {
        use std::fs::File;
        use std::io::BufReader;
        let file = File::open(path)?;
        let mut reader = tiff::decoder::Decoder::new(BufReader::new(file))
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let (w, h) = reader
            .dimensions()
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let desc = BufferDesc {
            width: w,
            height: h,
            planes: vec![PlaneDesc {
                offset: 0,
                stride: 4,
                row_stride: w as usize * 4,
                row_length: w,
                encoding: SampleFormat::U8,
            }],
            color_space: ColorSpace::SRGB,
            alpha_mode: AlphaMode::Premultiplied,
        };

        Ok(Self {
            path: path.to_path_buf(),
            width: w,
            height: h,
            color_space: ColorSpace::SRGB,
            layer_infos: vec![LayerFileInfo {
                name: path.file_stem().unwrap_or_default().to_string_lossy().into(),
                visible: true,
                opacity: 1.0,
                orientation: Orientation::Identity,
                offset: (0, 0),
                blend_mode: BlendMode::Normal,
                desc,
            }],
        })
    }
}
