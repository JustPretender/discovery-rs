use ratatui::{prelude::*, widgets::*};
use std::cell::RefCell;
use std::fmt::Display;

use crate::colors::*;

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
    items: Vec<Item>,
    state: RefCell<ListState>,
}

impl<Item> ListWidget<Item>
where
    Item: ListEntry + PartialEq,
{
    pub fn new() -> ListWidget<Item> {
        Self {
            items: vec![],
            state: Default::default(),
        }
    }
    pub fn selected(&self) -> Option<&Item> {
        self.items.get(self.state.borrow().selected().unwrap_or(0))
    }

    pub fn push(&mut self, item: Item) {
        if !self.items.contains(&item) {
            self.items.push(item);
        }
    }

    pub fn remove(&mut self, id: &String) {
        if let Some((index, _)) = self.items.iter().enumerate().find(|(_, el)| el.id() == *id) {
            self.items.remove(index);
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
    fn select_delta(&mut self, delta: isize) {
        // If there's nothing in the list, we can't do anything
        if !self.items.is_empty() {
            let index = match self.state.get_mut().selected() {
                Some(i) => (i as isize + delta).rem_euclid(self.items.len() as isize) as usize,
                // Nothing selected yet, pick the first item
                None => 0,
            };
            self.state.get_mut().select(Some(index));
        }
    }
}

impl<Item> Widget for &ListWidget<Item>
where
    Item: ListEntry,
{
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let inner_block = Block::new()
            .borders(Borders::NONE)
            .fg(TEXT_COLOR)
            .bg(NORMAL_ROW_COLOR);

        let items: Vec<_> = self
            .items
            .iter()
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
        StatefulWidget::render(list, area, buf, &mut self.state.borrow_mut());
    }
}
