use iced::widget::{column, container, row, scrollable, text, Space, button, mouse_area};
use iced::{Alignment, Background, Border, Color, Element, Length};
use crate::icons::{LUCIDE, SEARCH, SPARKLES};
use crate::theme::{
    BG_ELEVATED, BG_HOVER, BG_SURFACE, BORDER_SUBTLE, TEXT_MUTED, TEXT_PRIMARY,
    TEXT_SECONDARY,
};
use crate::modal::modal;
use crate::components::input::custom_input;

#[derive(Debug, Clone)]
pub enum Msg {
    Close,
    InputChanged(String),
    Hover(usize),
    Apply(usize),
}

#[derive(Clone, Debug)]
pub struct FilterItem {
    pub name: String,
    pub category: String,
}

pub struct FilterSearch {
    pub query: String,
    pub selected_index: usize,
    pub items: Vec<FilterItem>,
}

impl Default for FilterSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterSearch {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            selected_index: 2, // Default to Lens Blur as per UI screenshot
            items: vec![
                FilterItem { name: "Gaussian Blur".to_string(), category: "Blur".to_string() },
                FilterItem { name: "Motion Blur".to_string(), category: "Blur".to_string() },
                FilterItem { name: "Lens Blur".to_string(), category: "Blur".to_string() },
                FilterItem { name: "Radial Blur".to_string(), category: "Blur".to_string() },
            ],
        }
    }

    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::InputChanged(val) => {
                self.query = val;
            }
            Msg::Hover(idx) => self.selected_index = idx,
            Msg::Apply(idx) => {
                self.selected_index = idx;
            }
            _ => {}
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let search_bar = row![
            text(SEARCH).font(LUCIDE).size(16).color(TEXT_MUTED),
            Space::new().width(8.0),
            container(custom_input("Search...", &self.query, Msg::InputChanged))
                .width(Length::Fill),
        ]
        .align_y(Alignment::Center)
        .padding(iced::Padding { top: 0.0, right: 0.0, bottom: 16.0, left: 0.0 });

        let mut list_col = column![
            text(format!("FILTERS • {} MATCHES", self.items.len()))
                .size(10)
                .color(TEXT_MUTED)
                .font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..Default::default()
                }),
            Space::new().height(12.0),
        ];

        for (i, item) in self.items.iter().enumerate() {
            let is_selected = i == self.selected_index;
            let bg_color = if is_selected {
                Color::from_rgba(0.2, 0.6, 1.0, 0.1)
            } else {
                Color::TRANSPARENT
            };
            
            let active_indicator = if is_selected {
                container(Space::new())
                    .width(Length::Fixed(2.0))
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgb(0.2, 0.6, 1.0))),
                        ..Default::default()
                    })
            } else {
                container(Space::new())
                    .width(Length::Fixed(2.0))
                    .height(Length::Fill)
            };

            let query_lower = self.query.to_lowercase();
            let name_lower = item.name.to_lowercase();
            
            let name_el = if !query_lower.is_empty() && name_lower.contains(&query_lower) {
                let start = name_lower.find(&query_lower).unwrap();
                let end = start + query_lower.len();
                row![
                    text(&item.name[0..start]).size(14).color(TEXT_SECONDARY),
                    text(&item.name[start..end]).size(14).color(Color::from_rgb(0.2, 0.6, 1.0)),
                    text(&item.name[end..]).size(14).color(TEXT_SECONDARY),
                ]
            } else {
                row![text(&item.name).size(14).color(if is_selected { TEXT_PRIMARY } else { TEXT_SECONDARY })]
            };

            let item_btn = button(
                row![
                    active_indicator,
                    Space::new().width(12.0),
                    container(text(SPARKLES).font(LUCIDE).size(14).color(if is_selected { Color::from_rgb(0.2, 0.6, 1.0) } else { TEXT_MUTED }))
                        .width(24.0)
                        .height(24.0)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(if is_selected { Color::from_rgba(0.2, 0.6, 1.0, 0.1) } else { BG_ELEVATED })),
                            border: Border::default().rounded(4),
                            ..Default::default()
                        }),
                    Space::new().width(12.0),
                    name_el,
                    Space::new().width(Length::Fill),
                    text(&item.category).size(10).color(TEXT_MUTED).font(iced::Font {
                        family: iced::font::Family::Monospace,
                        ..Default::default()
                    }),
                    Space::new().width(16.0),
                ]
                .height(40.0)
                .align_y(Alignment::Center)
            )
            .width(Length::Fill)
            .padding(0)
            .style(move |_theme, status| {
                let hovered = matches!(status, iced::widget::button::Status::Hovered);
                iced::widget::button::Style {
                    background: Some(Background::Color(if hovered && !is_selected { BG_HOVER } else { bg_color })),
                    border: Border::default(),
                    ..Default::default()
                }
            })
            .on_press(Msg::Apply(i));
            
            let interactive_row = mouse_area(item_btn).on_enter(Msg::Hover(i));
            
            list_col = list_col.push(interactive_row);
        }

        let left_col = container(scrollable(list_col).width(Length::Fill))
            .width(Length::Fill)
            .padding(iced::Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 8.0 });

        let right_col = if let Some(item) = self.items.get(self.selected_index) {
            let preview_bg = match item.name.as_str() {
                "Lens Blur" => Color::from_rgb(0.7, 0.5, 0.6),
                "Gaussian Blur" => Color::from_rgb(0.4, 0.5, 0.8),
                _ => Color::from_rgb(0.5, 0.5, 0.5),
            };

            let preview = container(Space::new())
                .width(Length::Fill)
                .height(Length::Fixed(180.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(preview_bg)),
                    border: Border::default().rounded(6),
                    ..Default::default()
                });

            container(column![
                preview,
                Space::new().height(16.0),
                text(&item.name).size(14).color(TEXT_PRIMARY).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..Default::default()
                }),
                Space::new().height(8.0),
                text("Live preview on canvas.")
                    .size(12)
                    .color(TEXT_MUTED)
                    .line_height(iced::widget::text::LineHeight::Relative(1.4))
            ])
            .width(Length::Fixed(240.0))
            .padding(20)
            .style(|_| container::Style {
                background: Some(Background::Color(BG_SURFACE)),
                border: Border {
                    width: 1.0,
                    color: BORDER_SUBTLE,
                    ..Border::default()
                },
                ..Default::default()
            })
        } else {
            container(Space::new()).width(Length::Fixed(240.0))
        };

        let main_area = row![
            left_col,
            right_col,
        ]
        .height(Length::Fixed(320.0));

        let content = column![
            search_bar,
            main_area,
        ]
        .padding(20);

        modal("Search Filters", content)
            .width(640.0)
            .height(450.0)
            .on_close(Msg::Close)
            .into()
    }
}
