use ratatui::{prelude::*, widgets::*};
use std::cell::RefCell;
use std::fmt::Display;

use crate::colors::*;

#[derive(Debug)]
pub struct ListWidget<Item> {
    items: Vec<Item>,
    state: RefCell<ListState>,
}

impl<Item> ListWidget<Item>
where
    Item: Display + PartialEq,
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
                Some(i) => {
                    // Banking on the list not being longer than 2.4B items...
                    (i as isize + delta).rem_euclid(self.items.len() as isize) as usize
                }
                // Nothing selected yet, pick the first item
                None => 0,
            };
            self.state.get_mut().select(Some(index));
        }
    }
}

impl<Item> Widget for &ListWidget<Item>
where
    Item: Display,
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
                let line = Line::styled(format!("{}", item), TEXT_COLOR);
                ListItem::new(line).bg(if (index % 2) == 0 {
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
