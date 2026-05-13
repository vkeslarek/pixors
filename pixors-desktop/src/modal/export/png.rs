use iced::Element;
use iced::widget::Column;

use pixors_image::codec::PngCompression;

use super::components::*;
use super::presets::*;
use super::{ExportDialog, Msg};

pub fn png_options(dialog: &ExportDialog) -> Element<'_, Msg> {
    let cfg = &dialog.png;
    let preset = png_preset(cfg.compression);
    let show_level = matches!(cfg.compression, PngCompression::Level(_));
    let level_val = match cfg.compression {
        PngCompression::Level(l) => l as f32,
        _ => 6.0,
    };

    let mut compression_col = Column::new()
        .spacing(12)
        .push(section_label("COMPRESSION"))
        .push(labeled_pick(
            "Method",
            PNG_COMPRESSION_PRESETS,
            preset,
            Msg::PngCompressionPreset,
        ))
        .push(labeled_pick(
            "Filter",
            PNG_FILTERS,
            cfg.filter,
            Msg::PngFilter,
        ));

    if show_level {
        compression_col = compression_col.push(labeled_slider(
            "Level",
            1.0..=9.0,
            1.0,
            level_val,
            Msg::PngDeflateLevel,
        ));
    }

    let meta_col = Column::new()
        .spacing(12)
        .push(section_label("METADATA"))
        .push(labeled_checkbox(
            "Embed DPI",
            cfg.embed_dpi,
            Msg::PngEmbedDpi,
        ))
        .push(labeled_checkbox(
            "Embed ICC profile",
            cfg.embed_icc,
            Msg::PngEmbedIcc,
        ));

    Column::new()
        .spacing(24)
        .push(
            Column::new()
                .spacing(12)
                .push(section_label("IMAGE"))
                .push(labeled_pick(
                    "Color type",
                    PNG_COLOR_TYPES,
                    cfg.color_type,
                    Msg::PngColorType,
                ))
                .push(labeled_pick(
                    "Bit depth",
                    PNG_BIT_DEPTHS,
                    cfg.bit_depth,
                    Msg::PngBitDepth,
                ))
                .push(labeled_pick(
                    "Interlace",
                    PNG_INTERLACES,
                    cfg.interlace,
                    Msg::PngInterlace,
                )),
        )
        .push(compression_col)
        .push(meta_col)
        .into()
}
