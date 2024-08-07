use crate::colors::{HEADER_BG, NORMAL_ROW_COLOR, SEARCH_STYLE_BORDER, TEXT_COLOR};
use crate::widget::DiscoveryWidget;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::{Alignment, Constraint, Layout, Line, Stylize, Widget};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use regex::Regex;

#[derive(Debug, Default)]
pub struct Search {
    search: Option<String>,
}

impl Search {
    pub fn compile_regex(&self) -> anyhow::Result<Option<Regex>> {
        if let Some(search) = self.search.as_ref() {
            let regex = Regex::new(search)?;
            Ok(Some(regex))
        } else {
            Ok(None)
        }
    }
}

impl DiscoveryWidget for Search {
    fn title(&self) -> String {
        "Search".to_string()
    }

    fn controls(&self) -> String {
        "Use ↵ to apply. Esc to exit".to_string()
    }

    fn process_key_event(&mut self, key_event: &KeyEvent) {
        match (self.search.as_mut(), key_event.code) {
            (Some(regex), KeyCode::Char(c)) => {
                regex.push(c);
            }
            (Some(regex), KeyCode::Backspace) => {
                regex.pop();
            }
            (None, KeyCode::Char(c)) => {
                self.search = Some(c.to_string());
            }
            _ => {}
        }

        if self
            .search
            .as_ref()
            .filter(|search| !search.is_empty())
            .is_none()
        {
            self.search = None;
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer, _selected: bool)
    where
        Self: Sized,
    {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(SEARCH_STYLE_BORDER).bold())
            .title_alignment(Alignment::Center)
            .title(self.title())
            .title_style(Style::new().bold())
            .fg(TEXT_COLOR)
            .bg(HEADER_BG);
        let inner_area = block.inner(area);
        block.render(area, buf);

        let [search_area, footer_area] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(inner_area);
        let block = Block::new()
            .borders(Borders::NONE)
            .fg(TEXT_COLOR)
            .bg(NORMAL_ROW_COLOR);
        let input = Paragraph::new(Line::from(vec![
            Span::styled(" /", Style::default().fg(Color::DarkGray)),
            Span::from(self.search.as_deref().unwrap_or("")),
        ]))
        .block(block);

        Widget::render(input, search_area, buf);

        Paragraph::new(self.controls())
            .centered()
            .wrap(Wrap::default())
            .render(footer_area, buf);
    }
}
