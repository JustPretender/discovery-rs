use std::fmt::{Display, Formatter};
use mdns_sd::ServiceInfo;
use ratatui::{prelude::*, widgets::*};
use crate::colors::*;

#[derive(Debug)]
pub struct Info {
    pub info: ServiceInfo,
}

impl PartialEq for Info {
    fn eq(&self, other: &Self) -> bool {
        self.info.get_hostname() == other.info.get_hostname()
    }
}

impl Display for Info {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.info.get_hostname())
    }
}

impl Widget for &Info {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let block = Block::new()
            .borders(Borders::NONE)
            .padding(Padding::horizontal(1))
            .bg(NORMAL_ROW_COLOR);

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
                self.info.get_properties().to_string().into(),
            ]),
        ];
        let widths = [Constraint::Percentage(10), Constraint::Percentage(90)];

        let table = Table::new(rows, widths)
            .block(block)
            .column_spacing(1)
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::new().white())
            .on_black();

        Widget::render(table, area, buf);
    }
}
