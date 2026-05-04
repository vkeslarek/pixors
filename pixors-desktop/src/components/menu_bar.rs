use iced::widget::{button, container, row, text};
use iced::{Background, Border, Color, Element, Length, Shadow};

use iced_aw::menu::{self, Item, Menu};
use iced_aw::style::menu_bar::primary;
use iced_aw::style::status::Status;
use iced_aw::{menu_bar, menu_items};

use crate::theme::{
    ACCENT, BG_ELEVATED, BG_HOVER, BG_SURFACE, BORDER, BORDER_SUBTLE, MENUBAR_H,
    TEXT_PRIMARY, TEXT_SECONDARY,
};

#[derive(Debug, Clone)]
pub enum Msg {
    OpenFile,
    Exit,
    ZoomIn,
    ZoomOut,
    FitToScreen,
    ActualSize,
    ToggleLayers,
    ToggleFilters,
    ResetLayout,
    None,
}

pub fn view() -> Element<'static, Msg> {
    let logo = text("PIXORS").size(13).color(ACCENT);

    let mtpl = |items| Menu::new(items).max_width(220.0).offset(6.0).spacing(2.0);

    let mb = menu_bar!(
        (trigger("File"), mtpl(menu_items!(
            (item("Open\u{2026}", "Ctrl+O", Msg::OpenFile)),
            (separator()),
            (item("Exit", "Alt+F4", Msg::Exit)),
        )).width(200.0)),
        (trigger("View"), mtpl(menu_items!(
            (item("Zoom In", "Ctrl++", Msg::ZoomIn)),
            (item("Zoom Out", "Ctrl+-", Msg::ZoomOut)),
            (item("Fit to Screen", "Ctrl+0", Msg::FitToScreen)),
            (item("Actual Size", "Ctrl+1", Msg::ActualSize)),
        )).width(200.0)),
        (trigger("Window"), mtpl(menu_items!(
            (item("Toggle Layers", "", Msg::ToggleLayers)),
            (item("Toggle Filters", "", Msg::ToggleFilters)),
            (separator()),
            (item("Reset Layout", "", Msg::ResetLayout)),
        )).width(200.0)),
    )
    .draw_path(menu::DrawPath::Backdrop)
    .style(|theme: &iced::Theme, status: Status| menu::Style {
        path_border: Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        menu_background: Background::Color(BG_ELEVATED),
        menu_border: Border {
            color: BORDER,
            width: 1.0,
            radius: 6.0.into(),
        },
        bar_background: Background::Color(BG_SURFACE),
        bar_border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        path: Background::Color(BG_HOVER),
        ..primary(theme, status)
    });

    container(
        row![logo, mb]
            .spacing(16)
            .padding(iced::Padding::new(0.0).left(12).right(12))
            .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(MENUBAR_H)
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

fn trigger(label: &str) -> button::Button<'_, Msg, iced::Theme, iced::Renderer> {
    button(
        text(label)
            .size(12)
            .color(TEXT_SECONDARY)
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding([4, 10])
    .on_press(Msg::None)
    .style(|_, status| {
        let hovered = matches!(status, button::Status::Hovered);
        let bg = if hovered { BG_HOVER } else { Color::TRANSPARENT };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: if hovered { TEXT_PRIMARY } else { TEXT_SECONDARY },
            border: Border::default().rounded(4),
            ..Default::default()
        }
    })
}

fn item(
    label: &'static str,
    shortcut: &'static str,
    msg: Msg,
) -> button::Button<'static, Msg, iced::Theme, iced::Renderer> {
    let body: Element<'static, Msg> = if shortcut.is_empty() {
        text(label)
            .size(12)
            .color(TEXT_SECONDARY)
            .width(Length::Fill)
            .into()
    } else {
        row![
            text(label).size(12).color(TEXT_SECONDARY).width(Length::Fill),
            text(shortcut).size(11).color(crate::theme::TEXT_MUTED),
        ]
        .align_y(iced::Alignment::Center)
        .into()
    };

    button(body)
        .padding([5, 10])
        .width(Length::Fill)
        .on_press(msg)
        .style(|_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            let bg = if hovered { BG_HOVER } else { Color::TRANSPARENT };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: if hovered { TEXT_PRIMARY } else { TEXT_SECONDARY },
                border: Border::default().rounded(4),
                ..Default::default()
            }
        })
}

fn separator() -> Element<'static, Msg> {
    container(text(""))
        .width(Length::Fill)
        .height(1)
        .padding(iced::Padding::new(0.0).top(2).bottom(2))
        .style(|_| container::Style {
            background: Some(Background::Color(BORDER_SUBTLE)),
            ..Default::default()
        })
        .into()
}
