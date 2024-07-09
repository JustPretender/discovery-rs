use anyhow::Context;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use std::{error::Error, io::stdout};

use clap::Parser;
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
use parking_lot::Mutex;
use ratatui::{prelude::*, widgets::*};
use tracing::{instrument, Level};
use tracing_appender::non_blocking;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

use crate::info::Info;
use crate::list::ListWidget;
use crate::widget::DiscoveryWidget;

mod colors;
mod info;
mod list;
mod search;
mod utils;
mod widget;

#[derive(Parser, Debug, Default)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct CliOpts {
    #[arg(long)]
    /// mDNS service query, default: _services._dns-sd._udp.local.
    query: Option<String>,
    #[arg(long)]
    /// Interface to perform discovery on, default: All
    interface: Option<IfKind>,
    #[arg(long, action)]
    /// Enable tracing and debug logging
    tracing: bool,
}

const K_SERVICE_TYPE_ENUMERATION: &'static str = "_services._dns-sd._udp.local.";
const K_REFRESH_RATE: u8 = 24;

fn main() -> Result<(), Box<dyn Error>> {
    let opts = CliOpts::parse();

    init_error_hooks()?;

    // setup tracing and keep its guard
    let mut _tracing_guard = None;
    if opts.tracing {
        _tracing_guard = Some(init_tracing()?);
    }

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

/// Initialize the tracing subscriber to log to a file
///
/// This function initializes the tracing subscriber to log to a file named `tracing.log` in the
/// current directory. The function returns a [`WorkerGuard`] that must be kept alive for the
/// duration of the program to ensure that logs are flushed to the file on shutdown. The logs are
/// written in a non-blocking fashion to ensure that the logs do not block the main thread.
fn init_tracing() -> anyhow::Result<WorkerGuard> {
    let file = File::create("tracing.log").context("Failed to create tracing.log")?;
    let (non_blocking, guard) = non_blocking(file);

    // By default, the subscriber is configured to log all events with a level of `DEBUG` or higher,
    // but this can be changed by setting the `RUST_LOG` environment variable.
    let env_filter = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .init();
    Ok(guard)
}

#[derive(Debug, Default)]
enum Tab {
    #[default]
    Services,
    Instances,
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Running,
    Exit,
}

struct App {
    stop: Sender<()>,
    services: Arc<Mutex<ListWidget<String>>>,
    instances: Arc<Mutex<HashMap<String, ListWidget<Info>>>>,
    current_tab: Tab,
    worker_handle: Option<JoinHandle<anyhow::Result<()>>>,
}

impl App {
    #[instrument]
    fn new<T: AsRef<str> + std::fmt::Debug>(query: T, interface: IfKind) -> anyhow::Result<Self> {
        let mdns = ServiceDaemon::new()?;
        let mdns = Arc::new(Mutex::new(mdns));
        let services = Arc::new(Mutex::new(
            ListWidget::default().name("Services".to_string()),
        ));
        let instances = Arc::new(Mutex::new(HashMap::new()));
        let (stop_tx, stop_rx) = flume::bounded(1);

        let worker = {
            let mdns = mdns.clone();
            let services = services.clone();
            let instances = instances.clone();
            let query = query.as_ref().to_string();
            std::thread::spawn(move || -> anyhow::Result<()> {
                let _span = tracing::span!(Level::TRACE, "mDNS worker").entered();

                let base = {
                    let mdns = mdns.lock();
                    mdns.enable_interface(interface.clone())?;
                    mdns.browse(query.as_str())?
                };

                tracing::info!("Started the mDNS browsing");

                let receivers = Rc::new(RefCell::new(vec![base]));
                let event_handler = {
                    let receivers = receivers.clone();
                    let mdns = mdns.clone();
                    move |event| -> anyhow::Result<()> {
                        if let Ok(event) = event {
                            match event {
                                ServiceEvent::ServiceFound(service_type, full_name) => {
                                    tracing::debug!("New service found: {full_name}");
                                    if service_type == query {
                                        services.lock().push(full_name.clone());
                                        instances.lock().insert(
                                            full_name.clone(),
                                            ListWidget::default().name(full_name.clone()),
                                        );
                                        let receiver = mdns.lock().browse(&full_name)?;
                                        let mut receivers = receivers.borrow_mut();
                                        receivers.push(receiver);
                                    }
                                }
                                ServiceEvent::ServiceResolved(info) => {
                                    tracing::debug!("Service resolved: {info:#?}");
                                    if let Some(resolved) =
                                        instances.lock().get_mut(info.get_type())
                                    {
                                        resolved.push(Info { info });
                                    }
                                }
                                ServiceEvent::ServiceRemoved(service_type, full_name) => {
                                    tracing::debug!("Service removed: {full_name}");
                                    if service_type == query {
                                        services.lock().remove(&full_name);
                                        instances.lock().remove(&full_name);
                                    } else if let Some(resolved) =
                                        instances.lock().get_mut(&service_type)
                                    {
                                        resolved.remove(&full_name);
                                    }
                                }
                                ServiceEvent::SearchStarted(service) => {
                                    tracing::trace!("Search Started for {service}");
                                }
                                ServiceEvent::SearchStopped(service) => {
                                    tracing::trace!("Search Stopped for {service}");
                                }
                            }
                        }

                        Ok(())
                    }
                };

                let mut stop = false;
                while !stop {
                    let receivers = receivers.borrow().clone();
                    let mut selector = Selector::new();
                    for receiver in receivers.iter() {
                        selector = selector.recv(receiver, &event_handler);
                    }
                    selector = selector.recv(&stop_rx, |_| {
                        stop = true;
                        Ok(())
                    });
                    selector.wait()?;
                }

                mdns.lock().shutdown()?;

                tracing::info!("Stopped the mDNS browsing");

                Ok(())
            })
        };

        Ok(Self {
            services,
            instances,
            stop: stop_tx,
            current_tab: Tab::Services,
            worker_handle: Some(worker),
        })
    }

    fn handle_event(&mut self, event: Event) -> anyhow::Result<State> {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(State::Exit)
                    }
                    KeyCode::Left => self.current_tab = Tab::Services,
                    KeyCode::Right => self.current_tab = Tab::Instances,
                    _ => {
                        let mut services = self.services.lock();
                        let mut instances = self.instances.lock();

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

        Ok(State::Running)
    }

    fn run(&mut self, mut terminal: Terminal<impl Backend>) -> anyhow::Result<()> {
        loop {
            terminal.draw(|frame| {
                frame.render_widget(self as &mut App, frame.size());
            })?;

            if poll(Duration::from_millis(
                (K_REFRESH_RATE as f64 / 1000.) as u64,
            ))? {
                match self.handle_event(event::read()?)? {
                    State::Exit => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }

    fn shutdown(&mut self) -> anyhow::Result<()> {
        self.stop.send(())?;
        if let Some(handle) = self.worker_handle.take() {
            handle
                .join()
                .expect("The worker being joined has panicked")?;
        }
        Ok(())
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

        let services = self.services.lock();
        services.render(service_area, buf, matches!(self.current_tab, Tab::Services));
        if let Some(selected) = services.selected() {
            let instances = self.instances.lock();
            if let Some(resolved_instances) = instances.get(selected) {
                resolved_instances.render(
                    instances_area,
                    buf,
                    matches!(self.current_tab, Tab::Instances),
                );
                if let Some(info) = resolved_instances.selected() {
                    info.render(info_area, buf, false);
                }
            }
        }

        Paragraph::new(vec![
            Line::from(services.controls()),
            Line::from("←→ to switch panes, C-q to exit."),
        ])
        .centered()
        .render(footer_area, buf);
    }
}
