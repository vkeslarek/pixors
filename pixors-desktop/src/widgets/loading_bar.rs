use iced::widget::{container, row, text};
use iced::{Background, Element, Length};

pub fn loading_bar<'a, Msg: 'a>(loading: bool, progress: f32) -> Element<'a, Msg> {
    if loading {
        let done_pct = (progress.clamp(0.001, 1.0) * 10000.0) as u16;
        let remain_pct = 10000_u16.saturating_sub(done_pct).max(1);

        container(
            row![
                container(text(""))
                    .width(Length::FillPortion(done_pct))
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(crate::theme::OK_GREEN)),
                        ..Default::default()
                    }),
                container(text(""))
                    .width(Length::FillPortion(remain_pct))
                    .height(Length::Fill),
            ]
        )
        .width(Length::Fill)
        .height(3)
        .style(|_| container::Style {
            background: Some(Background::Color(crate::theme::BG_BASE)),
            ..Default::default()
        })
        .into()
    } else {
        container(text("")).height(0).into()
    }
}
