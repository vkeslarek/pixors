use std::path::PathBuf;
use std::sync::Arc;

use iced::Task;

use crate::app::{App, Msg, PaneKind};
use crate::page::menu_bar;

impl App {
    pub(crate) fn handle_menu_msg(&mut self, m: menu_bar::Msg) -> Task<Msg> {
        match m {
            menu_bar::Msg::Exit => std::process::exit(0),
            menu_bar::Msg::ToggleLayers => self.toggle_pane(PaneKind::Layers),
            menu_bar::Msg::ToggleFilters => self.toggle_pane(PaneKind::Filters),
            menu_bar::Msg::ShowUiShowcase => self.show_ui_showcase = true,
            menu_bar::Msg::ResetLayout => {
                self.panes = Self::default().panes;
            }
            menu_bar::Msg::OpenFile => return self.open_file_dialog(),
            menu_bar::Msg::Export if self.active_file_path().is_some() => {
                self.show_export_dialog = true;
            }
            _ => {}
        }
        Task::none()
    }

    pub(crate) fn handle_export_dialog(&mut self, m: crate::modal::export::Msg) -> Task<Msg> {
        match m {
            crate::modal::export::Msg::Export => {
                let config = self.export_dialog.encoder_config();
                let ext = self.export_dialog.file_extension();
                self.show_export_dialog = false;

                let Some(base_path) = self.active_file_path().map(|p| p.to_path_buf()) else {
                    return Task::none();
                };
                let suggested = base_path.with_extension(ext);
                let file_name = suggested
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("export")
                    .to_string();
                let ext = ext.to_string();

                self.pending_export = Some((config, ext.clone()));

                return Task::perform(
                    tokio::task::spawn_blocking(move || {
                        rfd::FileDialog::new()
                            .add_filter(ext.to_uppercase().as_str(), &[ext.as_str()])
                            .set_file_name(file_name.as_str())
                            .save_file()
                    }),
                    |result| Msg::ExportPathSelected(result.ok().flatten()),
                );
            }
            crate::modal::export::Msg::Cancel => {
                self.show_export_dialog = false;
            }
            other => self.export_dialog.update(other),
        }
        Task::none()
    }

    pub(crate) fn handle_export_path_selected(&mut self, save_path: Option<PathBuf>) {
        let Some(save_path) = save_path else {
            self.pending_export = None;
            return;
        };
        let Some((config, _)) = self.pending_export.take() else {
            return;
        };

        let Some(tab) = self.state.active_session_mut() else {
            return;
        };
        let session_id = tab.id;
        tab.transient.view.loading = true;
        tab.transient.view.progress = 0.0;

        let encoder_sink = match &config {
            pixors_image::codec::EncoderConfig::Png(png_cfg) => {
                pixors_engine::stage::Stage::Consumer(Box::new(
                    pixors_image::sink::png_encoder_v2::PngEncoderV2::new(
                        save_path.clone(),
                        png_cfg.clone(),
                        None,
                        None,
                    ),
                ))
            }
            pixors_image::codec::EncoderConfig::Tiff(tiff_cfg) => {
                pixors_engine::stage::Stage::Consumer(Box::new(
                    pixors_image::sink::tiff_encoder::TiffEncoderStage::new(
                        save_path.clone(),
                        tiff_cfg.clone(),
                        None,
                        None,
                    ),
                ))
            }
            pixors_image::codec::EncoderConfig::Jpeg(_jpeg_cfg) => {
                pixors_engine::stage::Stage::Consumer(Box::new(
                    pixors_image::sink::jpeg_encoder::JpegEncoderStage::new(
                        save_path.clone(),
                        &config,
                    ),
                ))
            }
            pixors_image::codec::EncoderConfig::WebP(_webp_cfg) => {
                pixors_engine::stage::Stage::Consumer(Box::new(
                    pixors_image::sink::webp_encoder::WebPEncoderStage::new(
                        save_path.clone(),
                        &config,
                    ),
                ))
            }
        };

        let compile_config = pixors_document::render::compiler::CompileConfig {
            disk_caches: tab.transient.disk_caches.clone(),
            cache_dir: tab.transient.cache_dir.clone(),
            display_format: tab.display_format,
            display_color_space: tab.display_color_space,
            working_format: tab.working_format,
            working_color_space: tab.working_color_space,
            tile_size: pixors_document::TILE_SIZE,
            img_w: tab.document.canvas.width,
            img_h: tab.document.canvas.height,
        };
        let graph = pixors_document::render::compiler::compile_export(
            &tab.document,
            &compile_config,
            encoder_sink,
        );
        let _ = self.dispatcher.run_graph(graph, Some(session_id));
    }

    pub(crate) fn handle_ui_showcase(&mut self, m: crate::modal::ui_showcase::Msg) {
        match m {
            crate::modal::ui_showcase::Msg::Close => self.show_ui_showcase = false,
            other => self.ui_showcase.update(other),
        }
    }

    pub(crate) fn handle_filter_search(&mut self, m: crate::modal::filter_search::Msg) {
        match m {
            crate::modal::filter_search::Msg::Close => self.show_filter_search = false,
            crate::modal::filter_search::Msg::Apply(idx) => {
                let op = self
                    .filter_search
                    .items
                    .get(idx)
                    .map(|item| item.op.clone())
                    .unwrap_or(pixors_document::Operation::Blur { radius: 5.0 });
                self.filter_search
                    .update(crate::modal::filter_search::Msg::Apply(idx));
                self.show_filter_search = false;

                if let (Some(tab), Some(layer_id)) = (
                    self.state.active_session(),
                    self.state
                        .active_session()
                        .and_then(|t| t.transient.active_node),
                ) {
                    let session_id = tab.id;
                    let new_id = self
                        .state
                        .session_mut(session_id)
                        .map(|t| t.document.alloc_node_id())
                        .unwrap_or(pixors_document::NodeId(0));
                    let _ = self.dispatcher.commit(
                        Arc::new(pixors_document::mutation::impls::AddTransform {
                            tab: session_id,
                            layer: layer_id,
                            transform: pixors_document::Transform {
                                id: new_id,
                                op,
                                input: pixors_document::InputScope::Layer,
                                output: pixors_document::OutputMode::Replace {
                                    blend: pixors_document::BlendSpec {
                                        mode: pixors_image::image::BlendMode::Normal,
                                        opacity: 1.0,
                                    },
                                },
                                enabled: true,
                            },
                        }),
                        &mut self.state,
                    );
                    self.recomposite_current_view(session_id);
                }
            }
            other => self.filter_search.update(other),
        }
    }

    pub(crate) fn open_file_dialog(&self) -> Task<Msg> {
        Task::perform(
            tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .add_filter("Images", &["png", "tiff", "tif", "jpg", "jpeg", "webp"])
                    .pick_file()
            }),
            |result| Msg::FileOpened(result.ok().flatten()),
        )
    }

    pub(crate) fn handle_file_opened(&mut self, path: PathBuf) {
        match pixors_document::ingest::prepare_ingest(&path, &mut self.state) {
            Ok(result) => {
                let session_id = result.session_id;
                self.state.push(result.session);
                self.pending_ingest = Some(session_id);
                let _ = self.dispatcher.run_graph(result.graph, Some(session_id));
                self.update_status_from_active_tab();
            }
            Err(e) => self.push_error(e),
        }
    }
}
