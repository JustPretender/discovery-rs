use crate::colors::*;
use crate::list::ListEntry;
use crate::widget::DiscoveryWidget;
use crossterm::event::KeyEvent;
use mdns_sd::ServiceInfo;
use ratatui::{prelude::*, widgets::*};

/// [`ServiceInfo`] wrapper.
///
/// Implements traits, necessary for the [`ServiceInfo`] to be
/// rendered either as a [`Widget`] or simply as an entry in the [`List`]
#[derive(Debug)]
pub struct Info {
    pub info: ServiceInfo,
}

impl PartialEq for Info {
    fn eq(&self, other: &Self) -> bool {
        self.info.get_hostname() == other.info.get_hostname()
    }
}

impl ListEntry for Info {
    fn entry(&self) -> Line {
        Line::styled(format!("{}", self.info.get_hostname()), TEXT_COLOR)
    }

    fn id(&self) -> String {
        self.info.get_hostname().to_string()
    }
}

impl DiscoveryWidget for &Info {
    fn title(&self) -> String {
        self.id()
    }

    fn controls(&self) -> String {
        "".to_string()
    }

    fn process_key_event(&mut self, _key_event: &KeyEvent) {}

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
            .padding(Padding::horizontal(1))
            .bg(NORMAL_ROW_COLOR);
        let properties = textwrap::wrap(
            &self.info.get_properties().to_string(),
            // Fit to end, minus "properties" and cell spacing
            textwrap::Options::new((area.width as usize).saturating_sub(10 + 1)),
        )
        .join("\n");
        let rows = [
            Row::new([
                Cell::new("Hostname").bold().light_cyan(),
                self.info.get_hostname().into(),
            ]),
            Row::new([
                Cell::new("Addresses").bold().light_cyan(),
                self.info
                    .get_addresses()
                    .into_iter()
                    .map(|addr| addr.to_string())
                    .fold(String::new(), |acc, addr| acc + &addr + " ")
                    .into(),
            ]),
            Row::new([
                Cell::new("Port").bold().light_cyan(),
                self.info.get_port().to_string().into(),
            ]),
            Row::new([
                Cell::new("Host TTL").bold().light_cyan(),
                self.info.get_host_ttl().to_string().into(),
            ]),
            Row::new([
                Cell::new("Other TTL").bold().light_cyan(),
                self.info.get_other_ttl().to_string().into(),
            ]),
            Row::new([
                Cell::new("Priority").bold().light_cyan(),
                self.info.get_priority().to_string().into(),
            ]),
            Row::new([
                Cell::new("Weight").bold().light_cyan(),
                self.info.get_weight().to_string().into(),
            ]),
            Row::new([
                Cell::new("Properties").bold().light_cyan(),
                Cell::new(properties),
            ])
            .height(2),
        ];
        let widths = [Constraint::Percentage(10), Constraint::Percentage(90)];

        let table = Table::new(rows, widths)
            .block(inner_block)
            .column_spacing(1)
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::new().white())
            .on_black();

        Widget::render(table, inner_area, buf);
    }
}
