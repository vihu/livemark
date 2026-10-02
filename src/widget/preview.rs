//! Split mode's preview (PLAN-002): the note rendered by iced's `markdown`
//! widget beside its source, parsed again after each edit. It is iced's
//! view of the note, not live preview's (front matter, `==highlight==` and
//! task boxes follow iced's widget). Its pictures are those the host gave
//! `Editor::set_image`; a link clicked in it goes to the host as
//! `Message::link`.
use std::collections::HashMap;

use iced::widget::{container, image, markdown, scrollable};
use iced::{Element, Length, Theme};

use super::picture::Picture;
use super::shape::TEXT_SIZE;
use super::{Editor, Input, Message, Mode};

/// The preview's parse and the document version it is of.
#[derive(Default)]
pub struct Preview {
    pub(super) content: markdown::Content,
    version: Option<u64>,
}

/// Turns parsed items into widgets, pictures and links included.
struct Viewer<'a> {
    theme: Theme,
    pictures: &'a HashMap<String, Picture>,
}

impl<'a> markdown::Viewer<'a, Message> for Viewer<'a> {
    fn theme(&self) -> &Theme {
        &self.theme
    }

    fn highlighter(&self) -> &dyn markdown::Highlighter<iced::Code, Theme> {
        markdown::Catalog::highlighter(&self.theme)
    }

    fn on_link_click(url: markdown::Uri) -> Message {
        Message(Input::Follow(url))
    }

    fn image(
        &self,
        settings: markdown::Settings,
        url: &'a markdown::Uri,
        _title: &'a str,
        alt: &markdown::Text,
    ) -> Element<'a, Message> {
        match self.pictures.get(url.as_str()) {
            Some(picture) => image(picture.handle.clone()).width(Length::Shrink).into(),
            // No picture: its alt text, as the widget shows one.
            None => container(
                iced::widget::rich_text(alt.spans(settings, &self.theme, self.highlighter()))
                    .on_link_click(Self::on_link_click),
            )
            .padding(settings.spacing.0)
            .into(),
        }
    }
}

impl Editor {
    /// Parses the text again for the preview when it changed, in Split
    /// mode.
    pub(super) fn refresh_preview(&mut self) {
        let version = self.doc.version();
        if self.mode == Mode::Split && self.preview.version != Some(version) {
            self.preview = Preview {
                content: markdown::Content::parse(self.doc.text()),
                version: Some(version),
            };
        }
    }

    /// The rendered note, scrolling on its own, in the theme the editor was
    /// last drawn in.
    pub(super) fn preview(&self) -> Element<'_, Message> {
        let theme = self.lines.borrow().theme.clone().unwrap_or(Theme::Light);
        let viewer = Viewer {
            theme,
            pictures: &self.pictures,
        };
        let settings = markdown::Settings::with_text_size(TEXT_SIZE * self.zoom());
        let items = self.preview.content.items();
        scrollable(container(markdown::view_with(items, settings, &viewer)).padding(16))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
