use iced::widget::{Column, row, text};
use iced::{Alignment, Element};

use pixors_image::codec::{TiffBitDepth, TiffCompression, TiffPredictor, TiffVariant};

use crate::theme::TEXT_SECONDARY;

use super::components::*;
use super::presets::*;
use super::{ExportDialog, Msg};

pub fn tiff_options(dialog: &ExportDialog) -> Element<'_, Msg> {
    let cfg = &dialog.tiff;
    let compression_kind = tiff_compression_kind(&cfg.compression);
    let layout_kind = tiff_layout_kind(&cfg.layout);
    let show_predictor = matches!(
        cfg.compression,
        TiffCompression::Lzw { .. } | TiffCompression::Deflate { .. }
    );
    let show_deflate_level = matches!(cfg.compression, TiffCompression::Deflate { .. });
    let deflate_level = current_deflate_level(&cfg.compression) as f32;
    let predictor = current_tiff_predictor(&cfg.compression);

    let predictor_options: &[TiffPredictor] = if cfg.bit_depth == TiffBitDepth::ThirtyTwo {
        TIFF_PREDICTORS_ALL
    } else {
        TIFF_PREDICTORS_INT
    };

    let mut compression_col = Column::new()
        .spacing(12)
        .push(section_label("COMPRESSION"))
        .push(labeled_pick(
            "Method",
            TIFF_COMPRESSION_KINDS,
            compression_kind,
            Msg::TiffCompressionKind,
        ));

    if show_predictor {
        compression_col = compression_col.push(labeled_pick(
            "Predictor",
            predictor_options,
            predictor,
            Msg::TiffPredictor,
        ));
    }
    if show_deflate_level {
        compression_col = compression_col.push(labeled_slider(
            "Level",
            1.0..=9.0,
            1.0,
            deflate_level,
            Msg::TiffDeflateLevel,
        ));
    }

    let mut layout_col = Column::new()
        .spacing(12)
        .push(section_label("LAYOUT"))
        .push(
            row![
                text("Mode").size(13).color(TEXT_SECONDARY).width(140),
                layout_toggle(layout_kind),
            ]
            .align_y(Alignment::Center),
        );

    match layout_kind {
        TiffLayoutKind::Strip => {
            layout_col = layout_col.push(labeled_text_input(
                "Rows per strip",
                &dialog.rows_per_strip_str,
                Msg::TiffRowsPerStrip,
            ));
        }
        TiffLayoutKind::Tile => {
            layout_col = layout_col
                .push(labeled_text_input(
                    "Tile width",
                    &dialog.tile_width_str,
                    Msg::TiffTileWidth,
                ))
                .push(labeled_text_input(
                    "Tile height",
                    &dialog.tile_height_str,
                    Msg::TiffTileHeight,
                ));
        }
    }

    layout_col = layout_col
        .push(labeled_pick(
            "Byte order",
            TIFF_BYTE_ORDERS,
            cfg.byte_order,
            Msg::TiffByteOrder,
        ))
        .push(labeled_checkbox(
            "BigTIFF",
            matches!(cfg.tiff_variant, TiffVariant::BigTiff),
            Msg::TiffBigTiff,
        ));

    let meta_col = Column::new()
        .spacing(12)
        .push(section_label("METADATA"))
        .push(labeled_checkbox(
            "Embed DPI",
            cfg.embed_dpi,
            Msg::TiffEmbedDpi,
        ))
        .push(labeled_checkbox(
            "Embed ICC profile",
            cfg.embed_icc,
            Msg::TiffEmbedIcc,
        ))
        .push(labeled_checkbox(
            "Embed EXIF",
            cfg.embed_exif,
            Msg::TiffEmbedExif,
        ))
        .push(labeled_checkbox(
            "Multipage",
            cfg.multipage,
            Msg::TiffMultipage,
        ));

    Column::new()
        .spacing(24)
        .push(
            Column::new()
                .spacing(12)
                .push(section_label("IMAGE"))
                .push(labeled_pick(
                    "Color type",
                    TIFF_COLOR_TYPES,
                    cfg.color_type,
                    Msg::TiffColorType,
                ))
                .push(labeled_pick(
                    "Bit depth",
                    TIFF_BIT_DEPTHS,
                    cfg.bit_depth,
                    Msg::TiffBitDepth,
                )),
        )
        .push(compression_col)
        .push(layout_col)
        .push(meta_col)
        .into()
}
