use iced::widget::{container, pane_grid, PaneGrid};
use iced::{Background, Border, Color, Element, Length};

use crate::ui::app::{Msg, PaneKind};
use crate::ui::theme::{ACCENT, BG_SURFACE, BORDER_SUBTLE, SIDEBAR_W};

pub fn view<'a>(
    panes: &'a pane_grid::State<PaneKind>,
    pane_content: impl Fn(pane_grid::Pane, PaneKind) -> pane_grid::Content<'a, Msg> + 'a,
) -> Element<'a, Msg> {
    if panes.iter().count() == 0 {
        return container(iced::widget::text(""))
            .width(0)
            .height(Length::Fill)
            .into();
    }

    let pg = PaneGrid::new(panes, |pane, kind, _focus| pane_content(pane, *kind))
        .spacing(0)
        .on_resize(8, Msg::PaneResized)
        .on_drag(Msg::PaneDragged)
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

    container(pg)
        .width(SIDEBAR_W)
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
