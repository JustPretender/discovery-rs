use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{prelude::*, widgets::*};
use regex::Regex;
use std::cell::RefCell;
use std::fmt::Display;
use tracing::instrument;

use crate::colors::*;
use crate::search::Search;
use crate::utils::centered_rect;
use crate::widget::DiscoveryWidget;

#[derive(Debug, Default)]
enum Mode {
    #[default]
    Display,
    Search,
}

/// Custom [`List`] entry trait
///
/// Implementing this trait for a type will make it possible
/// for the type to be rendered as a line in the [`List`].
pub trait ListEntry {
    fn entry(&self) -> Line;
    fn id(&self) -> String;
}

impl<D: Display> ListEntry for D {
    fn entry(&self) -> Line {
        Line::styled(format!("{}", self), TEXT_COLOR)
    }

    fn id(&self) -> String {
        format!("{}", self)
    }
}

/// Custom [`List`] widget.
///
/// Keeps track of the list elements and implements [`Widget`] so
/// that the list can be rendered as part of the TUI.
#[derive(Debug)]
pub struct ListWidget<Item> {
    name: String,
    items: Vec<Item>,
    state: RefCell<ListState>,
    search_regex: Option<Regex>,
    search: Search,
    current_mode: Mode,
}

impl<Item> Default for ListWidget<Item> {
    fn default() -> Self {
        Self {
            name: "ListWidget".to_string(),
            items: Default::default(),
            state: RefCell::new(ListState::default()),
            search: Search::default(),
            search_regex: None,
            current_mode: Mode::default(),
        }
    }
}

impl<Item> ListWidget<Item>
where
    Item: ListEntry + PartialEq + std::fmt::Debug,
{
    pub fn name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn selected(&self) -> Option<&Item> {
        let filtered = self.filtered();
        filtered
            .get(self.state.borrow().selected().unwrap_or(0))
            .copied()
    }

    pub fn push(&mut self, item: Item) {
        if !self.items.contains(&item) {
            self.items.push(item);
        }

        // Select the first item once we have at least one
        let state = self.state.get_mut();
        if state.selected().is_none() {
            state.select(Some(0));
        }
    }

    pub fn remove(&mut self, id: &String) {
        if let Some((index, _)) = self.items.iter().enumerate().find(|(_, el)| el.id() == *id) {
            self.items.remove(index);
        }

        // Deselect when all the items are gone
        if self.items.is_empty() {
            self.state.get_mut().select(None);
        }
    }

    pub fn next(&mut self) {
        self.select_delta(1);
    }

    pub fn prev(&mut self) {
        self.select_delta(-1);
    }

    pub fn top(&mut self) {
        let s = self.state.get_mut().selected().unwrap_or(0) as isize;
        self.select_delta(-1 * s);
    }

    pub fn bottom(&mut self) {
        let s = self.state.get_mut().selected().unwrap_or(0) as isize;
        self.select_delta(s);
    }

    /// Move some number of items up or down the list. Selection will wrap if
    /// it underflows/overflows.
    #[instrument]
    fn select_delta(&mut self, delta: isize) {
        tracing::trace!(
            "List state before the update: {:?}",
            self.state.get_mut().selected()
        );
        let filtered = self.filtered();
        // If there's nothing in the list, we can't do anything
        if !filtered.is_empty() {
            let len = filtered.len() as isize;
            let index = match self.state.get_mut().selected() {
                Some(i) => (i as isize + delta).rem_euclid(len) as usize,
                // Nothing selected yet, pick the first item
                None => 0,
            };
            self.state.get_mut().select(Some(index));
        }
        tracing::trace!(
            "List state after the update: {:?}",
            self.state.get_mut().selected()
        );
    }

    fn filtered(&self) -> Vec<&Item> {
        if let Some(regex) = self.search_regex.as_ref() {
            self.items
                .iter()
                .filter(|item| regex.is_match(&item.id()))
                .collect()
        } else {
            self.items.iter().collect()
        }
    }

    #[instrument]
    fn update_filter(&mut self, regex: Option<Regex>) {
        self.search_regex = regex;
        let filtered = self.filtered();
        if !filtered.is_empty() {
            self.state.get_mut().select(Some(0));
        }
        tracing::debug!("Filter has been updated");
    }
}

impl<Item> DiscoveryWidget for ListWidget<Item>
where
    Item: ListEntry + PartialEq + std::fmt::Debug,
{
    fn title(&self) -> String {
        format!(
            "{}{}",
            self.name,
            if let Some(regex) = self.search_regex.as_ref() {
                format!("(/{}/)", regex.to_string())
            } else {
                "".to_string()
            }
        )
    }

    fn controls(&self) -> String {
        "Use ↓↑ to select next/prev, g/G to go top/bottom, / to search".to_string()
    }

    fn process_key_event(&mut self, event: &KeyEvent) {
        match self.current_mode {
            Mode::Search => match event.code {
                KeyCode::Esc => {
                    self.current_mode = Mode::Display;
                }
                KeyCode::Enter => {
                    self.current_mode = Mode::Display;
                    self.update_filter(self.search.compile_regex().ok().flatten());
                }
                KeyCode::Char(_) | KeyCode::Backspace => {
                    self.search.process_key_event(event);
                }
                _ => {}
            },
            Mode::Display => match event.code {
                KeyCode::Down => self.next(),
                KeyCode::Up => self.prev(),
                KeyCode::Char('g') => self.top(),
                KeyCode::Char('G') => self.bottom(),
                KeyCode::Char('/') => self.current_mode = Mode::Search,
                _ => {}
            },
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer, selected: bool) {
        let outer_block = Block::new()
            .borders(Borders::ALL)
            .border_style(if selected {
                Style::new().fg(SELECTED_STYLE_FG)
            } else {
                Style::default()
            })
            .title_alignment(Alignment::Center)
            .title(self.title())
            .title_style(Style::new().bold())
            .fg(TEXT_COLOR)
            .bg(HEADER_BG);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let inner_block = Block::new()
            .borders(Borders::NONE)
            .fg(TEXT_COLOR)
            .bg(NORMAL_ROW_COLOR);

        let items: Vec<_> = self
            .items
            .iter()
            .filter(|item| {
                if let Some(regex) = self.search_regex.as_ref() {
                    regex.is_match(&item.id())
                } else {
                    true
                }
            })
            .enumerate()
            .map(|(index, item)| {
                ListItem::new(item.entry()).bg(if (index % 2) == 0 {
                    NORMAL_ROW_COLOR
                } else {
                    ALT_ROW_COLOR
                })
            })
            .collect();
        let list = List::new(items)
            .block(inner_block)
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED)
                    .fg(SELECTED_STYLE_FG),
            )
            .highlight_symbol(">")
            .highlight_spacing(HighlightSpacing::Always);
        StatefulWidget::render(list, inner_area, buf, &mut self.state.borrow_mut());

        if matches!(self.current_mode, Mode::Search) {
            let search_area = centered_rect(60, 20, area);
            Clear.render(search_area, buf);
            self.search.render(search_area, buf, true);
        }
    }
}
