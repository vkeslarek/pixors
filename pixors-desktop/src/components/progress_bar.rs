use crate::theme::{BG_BASE, OK_GREEN};
use iced::widget::{Space, container, row};
use iced::{Background, Color, Element, Length};

pub struct ProgressBar<'a, Message> {
    progress: f32,
    height: f32,
    color: Color,
    background_color: Color,
    _phantom: std::marker::PhantomData<&'a Message>,
}

pub fn progress_bar<'a, Message>(progress: f32) -> ProgressBar<'a, Message> {
    ProgressBar {
        progress: progress.clamp(0.0, 1.0),
        height: 4.0,
        color: OK_GREEN,
        background_color: BG_BASE,
        _phantom: std::marker::PhantomData,
    }
}

impl<'a, Message> ProgressBar<'a, Message> {
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn background_color(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }
}

impl<'a, Message: 'a> From<ProgressBar<'a, Message>> for Element<'a, Message> {
    fn from(p: ProgressBar<'a, Message>) -> Self {
        let done_pct = (p.progress * 10000.0).max(1.0) as u16;
        let remain_pct = 10000_u16.saturating_sub(done_pct).max(1);

        container(row![
            container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::FillPortion(done_pct))
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(p.color)),
                    border: iced::Border::default().rounded(2),
                    ..Default::default()
                }),
            container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::FillPortion(remain_pct))
                .height(Length::Fill),
        ])
        .width(Length::Fill)
        .height(Length::Fixed(p.height))
        .style(move |_| container::Style {
            background: Some(Background::Color(p.background_color)),
            border: iced::Border::default().rounded(2),
            ..Default::default()
        })
        .into()
    }
}
