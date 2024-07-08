#![allow(clippy::enum_glob_use, clippy::wildcard_imports)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::ops::Deref;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use std::{error::Error, io::stdout};

use clap::{Parser, ValueHint};
use clap_derive::Parser;
use color_eyre::config::HookBuilder;
use crossterm::event::KeyModifiers;
use crossterm::{
    event::{self, poll, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use flume::{Selector, Sender};
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent};
use ratatui::{prelude::*, widgets::*};

use crate::colors::*;
use crate::info::Info;
use crate::list::ListWidget;

mod colors;
mod info;
mod list;
mod search;
mod utils;

#[derive(Parser, Debug, Default)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct CliOpts {
    #[arg(long)]
    /// mDNS service query, default: _services._dns-sd._udp.local.
    query: Option<String>,
    #[arg(long, value_hint = ValueHint::FilePath)]
    /// Enable debug logging to a file
    log_to: Option<PathBuf>,
    #[arg(long)]
    /// Interface to perform discovery on, default: All
    interface: Option<IfKind>,
}

#[derive(Debug, Default)]
enum Tab {
    #[default]
    Services,
    Instances,
}

struct App {
    mdns: Arc<Mutex<ServiceDaemon>>,
    stop: Sender<()>,
    services: Arc<Mutex<ListWidget<String>>>,
    instances: Arc<Mutex<HashMap<String, ListWidget<Info>>>>,
    current_tab: Tab,
}

const K_SERVICE_TYPE_ENUMERATION: &'static str = "_services._dns-sd._udp.local.";

fn main() -> Result<(), Box<dyn Error>> {
    let opts = CliOpts::parse();

    // setup logging to a file
    if let Some(ref path) = opts.log_to {
        let log_file = Box::new(File::create(path)?);
        env_logger::Builder::from_default_env()
            .target(env_logger::Target::Pipe(log_file))
            .init();
    }

    // setup terminal
    init_error_hooks()?;
    let terminal = init_terminal()?;

    // create app and run it
    let mut app = App::new(
        opts.query
            .as_ref()
            .map(|q| q.as_str())
            .unwrap_or(K_SERVICE_TYPE_ENUMERATION),
        opts.interface.unwrap_or(IfKind::All),
    )?;
    app.run(terminal)?;
    app.shutdown()?;

    restore_terminal()?;

    Ok(())
}

fn init_error_hooks() -> color_eyre::Result<()> {
    let (panic, error) = HookBuilder::default().into_hooks();
    let panic = panic.into_panic_hook();
    let error = error.into_eyre_hook();
    color_eyre::eyre::set_hook(Box::new(move |e| {
        let _ = restore_terminal();
        error(e)
    }))?;
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        panic(info);
    }));
    Ok(())
}

fn init_terminal() -> color_eyre::Result<Terminal<impl Backend>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal() -> color_eyre::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

impl App {
    fn new<T: AsRef<str>>(query: T, interface: IfKind) -> anyhow::Result<Self> {
        let mdns = ServiceDaemon::new()?;
        mdns.enable_interface(interface.clone())?;
        let base = mdns.browse(query.as_ref())?;
        let mdns = Arc::new(Mutex::new(mdns));
        let services = Arc::new(Mutex::new(ListWidget::new()));
        let instances = Arc::new(Mutex::new(HashMap::new()));
        let (stop_tx, stop_rx) = flume::bounded(1);

        log::info!("Started mDNS browsing on {interface:#?}");
        {
            let mdns = mdns.clone();
            let services = services.clone();
            let instances = instances.clone();
            let query = query.as_ref().to_string();
            std::thread::spawn(move || -> anyhow::Result<()> {
                let receivers = Rc::new(RefCell::new(vec![base]));
                let event_handler = {
                    let receivers = receivers.clone();
                    move |event| {
                        if let Ok(event) = event {
                            match event {
                                ServiceEvent::ServiceFound(service_type, full_name) => {
                                    log::info!("Service found {full_name}");
                                    if service_type == query {
                                        services
                                            .lock()
                                            .expect("Failed to acquire the service lock")
                                            .push(full_name.clone());
                                        instances
                                            .lock()
                                            .expect("Failed to acquire the instances lock")
                                            .insert(full_name.clone(), ListWidget::new());
                                        let receiver = mdns
                                            .lock()
                                            .expect("Failed to acquire the service daemon lock")
                                            .browse(&full_name)
                                            .expect("Failed to start mDNS browsing");

                                        let mut receivers = receivers.borrow_mut();
                                        receivers.push(receiver);
                                    }
                                }
                                ServiceEvent::ServiceResolved(info) => {
                                    log::info!("Service resolved {info:#?}");
                                    if let Some(resolved) = instances
                                        .lock()
                                        .expect("Failed to acquire the service lock")
                                        .get_mut(info.get_type())
                                    {
                                        resolved.push(Info { info });
                                    }
                                }
                                ServiceEvent::ServiceRemoved(service_type, full_name) => {
                                    log::info!("Service removed: {full_name}");

                                    if service_type == query {
                                        services
                                            .lock()
                                            .expect("Failed to acquire the service lock")
                                            .remove(&full_name);
                                        instances
                                            .lock()
                                            .expect("Failed to acquire the instances lock")
                                            .remove(&full_name);
                                    } else if let Some(resolved) = instances
                                        .lock()
                                        .expect("Failed to acquire the instances lock")
                                        .get_mut(&service_type)
                                    {
                                        resolved.remove(&full_name);
                                    }
                                }
                                ServiceEvent::SearchStarted(service) => {
                                    log::debug!("Search Started for {service}");
                                }
                                ServiceEvent::SearchStopped(service) => {
                                    log::debug!("Search Stopped for {service}");
                                }
                            }
                        }
                    }
                };

                let stop = AtomicBool::new(false);
                while !stop.load(Ordering::Acquire) {
                    let receivers = receivers.borrow().clone();
                    let mut selector = Selector::new();
                    for r in receivers.iter() {
                        selector = selector.recv(r, &event_handler);
                    }
                    selector = selector.recv(&stop_rx, |_| {
                        stop.store(true, Ordering::SeqCst);
                    });
                    selector.wait();
                }

                Ok(())
            });
        }

        Ok(Self {
            mdns,
            services,
            instances,
            stop: stop_tx,
            current_tab: Tab::Services,
        })
    }

    fn run(&mut self, mut terminal: Terminal<impl Backend>) -> anyhow::Result<()> {
        loop {
            self.draw(&mut terminal)?;

            if poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                return Ok(())
                            }
                            KeyCode::Left => self.current_tab = Tab::Services,
                            KeyCode::Right => self.current_tab = Tab::Instances,
                            _ => {
                                let mut services =
                                    self.services.lock().expect("Failed to acquire lock");
                                let mut instances =
                                    self.instances.lock().expect("Failed to acquire lock");

                                match self.current_tab {
                                    Tab::Services => {
                                        services.process_key_event(&key);
                                    }
                                    Tab::Instances => {
                                        if let Some(selected) = services
                                            .selected()
                                            .and_then(|service| instances.get_mut(service))
                                        {
                                            selected.process_key_event(&key);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw(&mut self, terminal: &mut Terminal<impl Backend>) -> anyhow::Result<()> {
        terminal.draw(|f| {
            f.render_widget(self as &mut App, f.size());
        })?;
        Ok(())
    }

    fn shutdown(&mut self) -> anyhow::Result<()> {
        self.stop.send(())?;
        self.mdns
            .lock()
            .expect("Failed to acquire the lock")
            .shutdown()?;
        Ok(())
    }

    fn render_block<W: Widget>(
        &self,
        title: &str,
        widget: W,
        area: Rect,
        buf: &mut Buffer,
        selected: bool,
    ) {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(if selected {
                Style::new().fg(SELECTED_STYLE_FG)
            } else {
                Style::default()
            })
            .title_alignment(Alignment::Center)
            .title(title)
            .title_style(Style::new().bold())
            .fg(TEXT_COLOR)
            .bg(HEADER_BG);
        let inner_area = block.inner(area);
        block.render(area, buf);
        widget.render(inner_area, buf);
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(12),
            Constraint::Length(2),
        ]);
        let [header_area, list_area, info_area, footer_area] = vertical.areas(area);

        Paragraph::new(format!(
            "{}, v{}",
            env!("CARGO_PKG_DESCRIPTION"),
            env!("CARGO_PKG_VERSION")
        ))
        .bold()
        .centered()
        .render(header_area, buf);

        let list_layout =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]);
        let [service_area, instances_area] = list_layout.areas(list_area);

        let services = self.services.lock().expect("Failed to acquire the lock");

        self.render_block(
            &format!(
                "Services{}",
                if let Some(regex) = services.search_regex() {
                    format!("(/{}/)", regex.to_string())
                } else {
                    "".to_string()
                }
            ),
            services.deref(),
            service_area,
            buf,
            if let Tab::Services = self.current_tab {
                true
            } else {
                false
            },
        );
        if let Some(selected) = services.selected() {
            let instances = self.instances.lock().expect("Failed to acquire the lock");
            if let Some(resolved_instances) = instances.get(selected) {
                self.render_block(
                    &format!(
                        "Resolved instances{}",
                        if let Some(regex) = resolved_instances.search_regex() {
                            format!("(/{}/)", regex.to_string())
                        } else {
                            "".to_string()
                        }
                    ),
                    resolved_instances,
                    instances_area,
                    buf,
                    if let Tab::Instances = self.current_tab {
                        true
                    } else {
                        false
                    },
                );
                if let Some(selected) = resolved_instances.selected() {
                    self.render_block("Detailed info", selected, info_area, buf, false);
                }
            }
        } else {
            self.render_block(
                "Resolved instances",
                Paragraph::default(),
                instances_area,
                buf,
                if let Tab::Instances = self.current_tab {
                    true
                } else {
                    false
                },
            );
            self.render_block("Detailed info", Paragraph::default(), info_area, buf, false);
        }

        Paragraph::new(vec![
            ListWidget::<String>::controls(),
            Line::from("←→ to switch panes, C-q to exit."),
        ])
        .centered()
        .render(footer_area, buf);
    }
}
