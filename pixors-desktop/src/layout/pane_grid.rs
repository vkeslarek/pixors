use crate::theme::{ACCENT, BG_SURFACE, BORDER_SUBTLE};
use iced::widget::{PaneGrid, container, pane_grid};
use iced::{Background, Border, Color, Element, Length};

pub struct PaneGridLayout<'a, T, Message> {
    panes: &'a pane_grid::State<T>,
    view: Box<dyn Fn(pane_grid::Pane, &'a T, bool) -> pane_grid::Content<'a, Message> + 'a>,
    on_resize: Option<Box<dyn Fn(pane_grid::ResizeEvent) -> Message + 'a>>,
    on_drag: Option<Box<dyn Fn(pane_grid::DragEvent) -> Message + 'a>>,
    width: Length,
}

pub fn pane_grid_layout<'a, T, Message>(
    panes: &'a pane_grid::State<T>,
    view: impl Fn(pane_grid::Pane, &'a T, bool) -> pane_grid::Content<'a, Message> + 'a,
) -> PaneGridLayout<'a, T, Message> {
    PaneGridLayout {
        panes,
        view: Box::new(view),
        on_resize: None,
        on_drag: None,
        width: Length::Fill,
    }
}

impl<'a, T, Message> PaneGridLayout<'a, T, Message> {
    pub fn on_resize(mut self, on_resize: impl Fn(pane_grid::ResizeEvent) -> Message + 'a) -> Self {
        self.on_resize = Some(Box::new(on_resize));
        self
    }

    pub fn on_drag(mut self, on_drag: impl Fn(pane_grid::DragEvent) -> Message + 'a) -> Self {
        self.on_drag = Some(Box::new(on_drag));
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
}

impl<'a, T, Message: 'a> From<PaneGridLayout<'a, T, Message>> for Element<'a, Message> {
    fn from(layout: PaneGridLayout<'a, T, Message>) -> Self {
        if layout.panes.iter().count() == 0 {
            return container(iced::widget::text(""))
                .width(0)
                .height(Length::Fill)
                .into();
        }

        let mut pg = PaneGrid::new(layout.panes, layout.view)
            .spacing(0)
            .style(|_| pane_grid::Style {
                hovered_region: pane_grid::Highlight {
                    background: Background::Color(Color::from_rgba(
                        ACCENT.r, ACCENT.g, ACCENT.b, 0.30,
                    )),
                    border: Border {
                        color: ACCENT,
                        width: 2.0,
                        radius: 4.0.into(),
                    },
                },
                picked_split: pane_grid::Line {
                    color: ACCENT,
                    width: 2.0,
                },
                hovered_split: pane_grid::Line {
                    color: ACCENT,
                    width: 2.0,
                },
            });

        if let Some(on_resize) = layout.on_resize {
            pg = pg.on_resize(8, on_resize);
        }

        if let Some(on_drag) = layout.on_drag {
            pg = pg.on_drag(on_drag);
        }

        container(pg)
            .width(layout.width)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(BG_SURFACE)),
                border: Border {
                    width: 0.0,
                    color: BORDER_SUBTLE,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
