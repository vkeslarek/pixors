use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Element, Length, Padding};
use crate::theme::{BG_BASE, BG_SURFACE, BORDER_SUBTLE, TEXT_PRIMARY};
use crate::components::*;
use crate::dialog;

#[derive(Debug, Clone)]
pub enum Msg {
    Close,
    SliderChanged(f32),
    SwitchToggled(bool),
    ButtonClicked,
    InputChanged(String),
    DropdownSelected(String),
}

#[derive(Default)]
pub struct UiShowcase {
    pub slider_value: f32,
    pub switch_value: bool,
    pub input_value: String,
    pub dropdown_selection: Option<String>,
}

impl UiShowcase {
    pub fn new() -> Self {
        Self {
            slider_value: 50.0,
            switch_value: true,
            input_value: String::new(),
            dropdown_selection: Some("Option 1".to_string()),
        }
    }

    pub fn update(&mut self, message: Msg) {
        match message {
            Msg::SliderChanged(val) => self.slider_value = val,
            Msg::SwitchToggled(val) => self.switch_value = val,
            Msg::InputChanged(val) => self.input_value = val,
            Msg::DropdownSelected(val) => self.dropdown_selection = Some(val),
            _ => {}
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let content = scrollable(
            column![
                // Badges
                card(
                    row![
                        badge("Info").variant(BadgeVariant::Info),
                        badge("Success").variant(BadgeVariant::Success),
                        badge("Warning").variant(BadgeVariant::Warning),
                        badge("Danger").variant(BadgeVariant::Danger),
                        badge("Neutral").variant(BadgeVariant::Neutral),
                    ]
                    .spacing(8)
                ).title("Badges"),

                // Buttons
                card(
                    column![
                        row![
                            button("Primary").variant(ButtonVariant::Primary).size(ButtonSize::Md).on_press(Msg::ButtonClicked),
                            button("Secondary").variant(ButtonVariant::Secondary).size(ButtonSize::Md).on_press(Msg::ButtonClicked),
                            button("Ghost").variant(ButtonVariant::Ghost).size(ButtonSize::Md).on_press(Msg::ButtonClicked),
                            button("Danger").variant(ButtonVariant::Danger).size(ButtonSize::Md).on_press(Msg::ButtonClicked),
                        ].spacing(8),
                        row![
                            button("Small").variant(ButtonVariant::Primary).size(ButtonSize::Sm).on_press(Msg::ButtonClicked),
                            button("Medium").variant(ButtonVariant::Primary).size(ButtonSize::Md).on_press(Msg::ButtonClicked),
                            button("Large").variant(ButtonVariant::Primary).size(ButtonSize::Lg).on_press(Msg::ButtonClicked),
                        ].spacing(8).align_y(Alignment::Center),
                        row![
                            button("Disabled").variant(ButtonVariant::Primary).size(ButtonSize::Md),
                        ],
                        row![
                            icon_button(crate::icons::EYE).size(16).on_press(Msg::ButtonClicked),
                            icon_button(crate::icons::TRASH).size(16).on_press(Msg::ButtonClicked),
                            icon_button(crate::icons::INFO).size(16).on_press(Msg::ButtonClicked),
                        ].spacing(8).align_y(Alignment::Center),
                    ].spacing(16)
                ).title("Buttons (Variants & Sizes)"),

                // Inputs
                card(
                    column![
                        slider(
                            "Opacity",
                            self.slider_value,
                            0.0..=100.0,
                            Msg::SliderChanged
                        ).value_format(|v| format!("{:.0}%", v)),
                        divider(),
                        switch(
                            "Hardware Acceleration",
                            self.switch_value,
                            Msg::SwitchToggled
                        ).description("Use GPU for rendering where possible"),
                        divider(),
                        input("Type something...", &self.input_value, Msg::InputChanged),
                        dropdown(
                            vec!["Option 1".to_string(), "Option 2".to_string(), "Option 3".to_string()],
                            self.dropdown_selection.clone(),
                            Msg::DropdownSelected
                        )
                    ].spacing(16)
                ).title("Inputs"),
            ]
            .spacing(16)
            .padding(20),
        );

        dialog("UI Components Showcase", content)
            .width(600.0)
            .height(500.0)
            .on_close(Msg::Close)
            .into()
    }
}
