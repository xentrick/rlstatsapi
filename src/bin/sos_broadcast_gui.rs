use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use eframe::egui::{self, Color32, CornerRadius, Frame, RichText, Stroke};
use futures_util::{SinkExt, StreamExt};
use rlstatsapi::{
    ClientOptions, RocketLeagueStatsClient,
    translate_stats_event,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

const FIXED_RL_HOST: &str = "127.0.0.1";
const FIXED_RL_PORT: u16 = 49123;
const FIXED_WS_HOST: &str = "0.0.0.0";
const FIXED_WS_PORT: u16 = 49122;
const FIXED_RECONNECT_MS: u64 = 500;
const OUTER_MARGIN: i8 = 24;
const SECTION_GAP: f32 = 14.0;
const DEFAULT_WINDOW_SIZE: [f32; 2] = [1060.0, 760.0];
const MIN_WINDOW_SIZE: [f32; 2] = [860.0, 620.0];

#[derive(Debug, Clone)]
struct BroadcastConfig {
    ini_path: Option<PathBuf>,
    rl_host: String,
    rl_port: u16,
    ws_host: String,
    ws_port: u16,
    reconnect_ms: u64,
    max_events: Option<usize>,
    debug: bool,
}

#[derive(Debug)]
enum WorkerEvent {
    Started { source: String, ws_url: String },
    Clients(usize),
    Relayed { total: usize },
    Log(String),
    Stopped(Result<(), String>),
}

struct SosBroadcastGuiApp {
    running: bool,
    status: String,
    source_addr: String,
    ws_url: String,
    client_count: usize,
    relayed_count: usize,
    logs: VecDeque<String>,
    worker_rx: Option<Receiver<WorkerEvent>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl SosBroadcastGuiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.text_edit_width = 260.0;

        style.visuals = egui::Visuals::dark();
        style.visuals.override_text_color = Some(Color32::from_rgb(228, 231, 242));
        style.visuals.panel_fill = Color32::from_rgb(15, 18, 24);
        style.visuals.window_fill = Color32::from_rgb(15, 18, 24);
        style.visuals.faint_bg_color = Color32::from_rgb(23, 28, 36);
        style.visuals.extreme_bg_color = Color32::from_rgb(10, 12, 16);
        style.visuals.selection.bg_fill = Color32::from_rgb(40, 140, 220);
        style.visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(220, 238, 255));

        style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(23, 28, 36);
        style.visuals.widgets.noninteractive.fg_stroke =
            Stroke::new(1.0, Color32::from_rgb(201, 209, 228));

        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(28, 34, 44);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(20, 25, 32);
        style.visuals.widgets.inactive.fg_stroke =
            Stroke::new(1.0, Color32::from_rgb(221, 227, 245));

        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 48, 62);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(32, 39, 50);
        style.visuals.widgets.hovered.fg_stroke =
            Stroke::new(1.0, Color32::from_rgb(236, 240, 255));
        style.visuals.widgets.hovered.bg_stroke =
            Stroke::new(1.0, Color32::from_rgb(68, 163, 255));

        style.visuals.widgets.active.bg_fill = Color32::from_rgb(52, 108, 186);
        style.visuals.widgets.active.fg_stroke =
            Stroke::new(1.0, Color32::from_rgb(247, 249, 255));
        style.visuals.widgets.active.bg_stroke =
            Stroke::new(1.0, Color32::from_rgb(105, 190, 255));

        style.visuals.widgets.open.bg_fill = Color32::from_rgb(40, 48, 62);
        style.visuals.window_corner_radius = CornerRadius::same(12);
        style.visuals.menu_corner_radius = CornerRadius::same(10);
        style.visuals.widgets.noninteractive.corner_radius =
            CornerRadius::same(8);
        style.visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
        style.visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
        style.visuals.widgets.active.corner_radius = CornerRadius::same(8);
        style.visuals.widgets.open.corner_radius = CornerRadius::same(8);
        cc.egui_ctx.set_style(style);

        Self {
            running: false,
            status: "Idle".to_string(),
            source_addr: format!("{}:{}", FIXED_RL_HOST, FIXED_RL_PORT),
            ws_url: format!("ws://{}:{}", FIXED_WS_HOST, FIXED_WS_PORT),
            client_count: 0,
            relayed_count: 0,
            logs: VecDeque::new(),
            worker_rx: None,
            shutdown_tx: None,
            worker_handle: None,
        }
    }

    fn push_log(&mut self, message: impl Into<String>) {
        const MAX_LOG_LINES: usize = 300;
        self.logs.push_back(message.into());
        while self.logs.len() > MAX_LOG_LINES {
            self.logs.pop_front();
        }
    }

    fn start(&mut self) {
        if self.running {
            return;
        }

        let config = Self::fixed_config();
        self.status = "Starting".to_string();
        self.source_addr = format!("{}:{}", config.rl_host, config.rl_port);
        self.ws_url = format!("ws://{}:{}", config.ws_host, config.ws_port);
        self.client_count = 0;
        self.relayed_count = 0;
        self.logs.clear();
        self.push_log(format!(
            "Starting SOS broadcaster (source={} ws={})",
            self.source_addr, self.ws_url
        ));

        let (worker_tx, worker_rx) = mpsc::channel::<WorkerEvent>();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let handle = thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = worker_tx.send(WorkerEvent::Stopped(Err(format!(
                        "Failed to start runtime: {error}"
                    ))));
                    return;
                }
            };

            let result = runtime.block_on(run_broadcast_worker(
                config,
                shutdown_rx,
                worker_tx.clone(),
            ));

            let _ = worker_tx.send(WorkerEvent::Stopped(result.map_err(|e| e.to_string())));
        });

        self.worker_rx = Some(worker_rx);
        self.shutdown_tx = Some(shutdown_tx);
        self.worker_handle = Some(handle);
        self.running = true;
    }

    fn stop(&mut self) {
        if let Some(tx) = &self.shutdown_tx {
            self.status = "Stopping".to_string();
            let _ = tx.send(true);
            self.push_log("Stopping broadcaster...".to_string());
        }
    }

    fn drain_events(&mut self) {
        let mut pending = Vec::new();
        if let Some(rx) = &self.worker_rx {
            while let Ok(event) = rx.try_recv() {
                pending.push(event);
            }
        }

        for event in pending {
            match event {
                WorkerEvent::Started { source, ws_url } => {
                    self.status = "Running".to_string();
                    self.source_addr = source;
                    self.ws_url = ws_url;
                    self.push_log(format!(
                        "Broadcast server ready at {}",
                        self.ws_url
                    ));
                }
                WorkerEvent::Clients(clients) => {
                    self.client_count = clients;
                }
                WorkerEvent::Relayed { total } => {
                    self.relayed_count = total;
                }
                WorkerEvent::Log(message) => {
                    self.push_log(message);
                }
                WorkerEvent::Stopped(result) => {
                    self.running = false;
                    self.shutdown_tx = None;
                    self.worker_rx = None;
                    if let Some(handle) = self.worker_handle.take() {
                        let _ = handle.join();
                    }

                    match result {
                        Ok(()) => {
                            self.status = "Stopped".to_string();
                            self.push_log("Broadcaster stopped".to_string());
                        }
                        Err(error) => {
                            self.status = "Error".to_string();
                            self.push_log(format!("Broadcaster stopped with error: {error}"));
                        }
                    }
                }
            }
        }
    }

    fn status_color(&self) -> Color32 {
        match self.status.as_str() {
            "Running" => Color32::from_rgb(86, 209, 153),
            "Error" => Color32::from_rgb(241, 110, 98),
            "Starting" => Color32::from_rgb(247, 194, 96),
            "Stopping" => Color32::from_rgb(224, 187, 108),
            _ => Color32::from_rgb(171, 177, 196),
        }
    }

    fn fixed_config() -> BroadcastConfig {
        BroadcastConfig {
            ini_path: None,
            rl_host: FIXED_RL_HOST.to_string(),
            rl_port: FIXED_RL_PORT,
            ws_host: FIXED_WS_HOST.to_string(),
            ws_port: FIXED_WS_PORT,
            reconnect_ms: FIXED_RECONNECT_MS,
            max_events: None,
            debug: false,
        }
    }

    fn card_frame(fill: Color32) -> Frame {
        Frame::default()
            .fill(fill)
            .stroke(Stroke::new(1.0, Color32::from_rgb(53, 63, 82)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(10))
    }

    fn banner_frame(&self) -> Frame {
        Frame::default()
            .fill(Color32::from_rgb(20, 27, 39))
            .stroke(Stroke::new(1.0, Color32::from_rgb(70, 91, 126)))
            .corner_radius(CornerRadius::same(16))
            .inner_margin(egui::Margin::same(18))
    }

    fn render_banner_chip(
        ui: &mut egui::Ui,
        label: &str,
        value: String,
        fill: Color32,
        stroke: Color32,
    ) {
        Frame::default()
            .fill(fill)
            .stroke(Stroke::new(1.0, stroke))
            .corner_radius(CornerRadius::same(255))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(label)
                            .color(Color32::from_rgb(170, 188, 221))
                            .strong(),
                    );
                    ui.label(
                        RichText::new(value)
                            .color(Color32::from_rgb(223, 232, 249))
                            .strong(),
                    );
                });
            });
    }

    fn render_hero_banner(&self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let title_size = if width > 1200.0 {
            44.0
        } else if width > 920.0 {
            39.0
        } else if width > 760.0 {
            34.0
        } else {
            28.0
        };
        let subtitle_size = if width > 860.0 { 23.0 } else { 18.0 };
        let status_fill = match self.status.as_str() {
            "Running" => Color32::from_rgb(25, 74, 60),
            "Error" => Color32::from_rgb(92, 35, 42),
            "Starting" => Color32::from_rgb(86, 62, 26),
            "Stopping" => Color32::from_rgb(78, 56, 23),
            _ => Color32::from_rgb(46, 58, 79),
        };

        self.banner_frame().show(ui, |ui| {
            ui.set_min_height(138.0);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("SOS Broadcaster")
                            .color(Color32::from_rgb(232, 239, 255))
                            .strong()
                            .size(title_size),
                    );
                    ui.label(
                        RichText::new(
                            "Translate RL Stats API events to SOS websocket messages with one click.",
                        )
                        .size(subtitle_size)
                        .color(Color32::from_rgb(164, 180, 210)),
                    );
                });

                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Min),
                    |ui| {
                        Frame::default()
                            .fill(status_fill)
                            .stroke(Stroke::new(1.0, self.status_color()))
                            .corner_radius(CornerRadius::same(255))
                            .inner_margin(egui::Margin::symmetric(14, 8))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("Status: {}", self.status))
                                        .strong()
                                        .color(self.status_color()),
                                );
                            });
                    },
                );
            });

            ui.add_space(10.0);

            ui.horizontal_wrapped(|ui| {
                Self::render_banner_chip(
                    ui,
                    "RL",
                    format!("{}:{}", FIXED_RL_HOST, FIXED_RL_PORT),
                    Color32::from_rgb(24, 39, 60),
                    Color32::from_rgb(70, 122, 189),
                );
                Self::render_banner_chip(
                    ui,
                    "WS",
                    self.ws_url.clone(),
                    Color32::from_rgb(31, 39, 51),
                    Color32::from_rgb(149, 116, 52),
                );
                Self::render_banner_chip(
                    ui,
                    "Clients",
                    self.client_count.to_string(),
                    Color32::from_rgb(29, 37, 53),
                    Color32::from_rgb(84, 150, 127),
                );
                Self::render_banner_chip(
                    ui,
                    "Relayed",
                    self.relayed_count.to_string(),
                    Color32::from_rgb(29, 37, 53),
                    Color32::from_rgb(104, 134, 206),
                );
            });
        });
    }

    fn render_rl_stats_card(&self, ui: &mut egui::Ui) {
        ui.set_min_height(108.0);

        let connection_text = if self.status == "Running" {
            "Connected"
        } else if self.status == "Starting" {
            "Connecting"
        } else if self.status == "Stopping" {
            "Stopping"
        } else {
            "Disconnected"
        };

        ui.label(
            RichText::new("RL Stats API")
                .color(Color32::from_rgb(130, 188, 255))
                .strong(),
        );
        ui.label(format!("Endpoint: {}:{}", FIXED_RL_HOST, FIXED_RL_PORT));
        ui.label(format!("State: {}", connection_text));
    }

    fn render_ws_stats_card(&self, ui: &mut egui::Ui) {
        ui.set_min_height(108.0);

        ui.label(
            RichText::new("Websocket Clients")
                .color(Color32::from_rgb(224, 187, 108))
                .strong(),
        );
        ui.label(format!("Bind: {}", self.ws_url));
        ui.label(format!("Clients: {}", self.client_count));
        ui.label(format!("Relayed: {}", self.relayed_count));
    }
}

impl Drop for SosBroadcastGuiApp {
    fn drop(&mut self) {
        if self.running {
            self.stop();
        }

        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

impl eframe::App for SosBroadcastGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        if self.running {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        egui::TopBottomPanel::top("header")
            .frame(Frame::default().fill(Color32::from_rgb(13, 16, 22)))
            .show(ctx, |ui| {
                Frame::default()
                    .inner_margin(egui::Margin::same(OUTER_MARGIN))
                    .show(ui, |ui| {
                        self.render_hero_banner(ui);
                });
            });

        egui::CentralPanel::default()
            .frame(Frame::default().fill(Color32::from_rgb(15, 18, 24)))
            .show(ctx, |ui| {
                Frame::default()
                    .inner_margin(egui::Margin::same(OUTER_MARGIN))
                    .show(ui, |ui| {
                        ui.add_space(2.0);

                        Self::card_frame(Color32::from_rgb(19, 24, 32))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_enabled_ui(!self.running, |ui| {
                                        if ui
                                            .add_sized(
                                                [120.0, 30.0],
                                                egui::Button::new(
                                                    RichText::new("Start").color(Color32::WHITE),
                                                )
                                                .corner_radius(CornerRadius::same(8))
                                                .fill(Color32::from_rgb(33, 134, 122)),
                                            )
                                            .clicked()
                                        {
                                            self.start();
                                        }
                                    });

                                    ui.add_enabled_ui(self.running, |ui| {
                                        if ui
                                            .add_sized(
                                                [120.0, 30.0],
                                                egui::Button::new(
                                                    RichText::new("Stop").color(Color32::WHITE),
                                                )
                                                .corner_radius(CornerRadius::same(8))
                                                .fill(Color32::from_rgb(160, 68, 76)),
                                            )
                                            .clicked()
                                        {
                                            self.stop();
                                        }
                                    });

                                    ui.separator();

                                    let status_fill = match self.status.as_str() {
                                        "Running" => Color32::from_rgb(27, 62, 52),
                                        "Error" => Color32::from_rgb(74, 31, 34),
                                        "Starting" => Color32::from_rgb(77, 57, 24),
                                        "Stopping" => Color32::from_rgb(68, 49, 24),
                                        _ => Color32::from_rgb(34, 41, 54),
                                    };

                                    Frame::default()
                                        .fill(status_fill)
                                        .stroke(Stroke::new(1.0, self.status_color()))
                                        .corner_radius(CornerRadius::same(255))
                                        .inner_margin(egui::Margin::symmetric(12, 5))
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(format!("Status: {}", self.status))
                                                    .strong()
                                                    .color(self.status_color()),
                                            );
                                        });
                                });
                            });

                        ui.add_space(SECTION_GAP);

                        ui.columns(2, |columns| {
                            Self::card_frame(Color32::from_rgb(19, 24, 32))
                                .show(&mut columns[0], |ui| {
                                    self.render_rl_stats_card(ui);
                                });

                            Self::card_frame(Color32::from_rgb(19, 24, 32))
                                .show(&mut columns[1], |ui| {
                                    self.render_ws_stats_card(ui);
                                });
                        });

                        ui.add_space(SECTION_GAP);
                        let available_log_space = ui.available_height();

                        Self::card_frame(Color32::from_rgb(19, 24, 32))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("Log")
                                        .color(Color32::from_rgb(130, 188, 255))
                                        .strong(),
                                );

                                // Reserve space for title/padding so the log panel does not overflow and clip at the bottom.
                                let inner_log_height = (available_log_space - 48.0).max(0.0);
                                let log_height = if self.running {
                                    inner_log_height.min(420.0)
                                } else {
                                    inner_log_height
                                };

                                Frame::default()
                                    .fill(Color32::from_rgb(12, 16, 22))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(46, 57, 76)))
                                    .corner_radius(CornerRadius::same(10))
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.set_min_height(log_height);
                                        egui::ScrollArea::vertical()
                                            .auto_shrink([false, false])
                                            .stick_to_bottom(true)
                                            .show(ui, |ui| {
                                                if self.logs.is_empty() {
                                                    ui.label(
                                                        RichText::new("Waiting for logs...")
                                                            .monospace()
                                                            .color(Color32::from_rgb(
                                                                120, 134, 163,
                                                            )),
                                                    );
                                                }

                                                for line in &self.logs {
                                                    ui.label(
                                                        RichText::new(line)
                                                            .monospace()
                                                            .color(Color32::from_rgb(
                                                                188, 199, 224,
                                                            )),
                                                    );
                                                }
                                            });
                                    });
                            });
                    });
            });
    }
}

fn emit_log(event_tx: &Sender<WorkerEvent>, message: impl Into<String>) {
    let _ = event_tx.send(WorkerEvent::Log(message.into()));
}

fn emit_debug(
    config: &BroadcastConfig,
    event_tx: &Sender<WorkerEvent>,
    message: impl Into<String>,
) {
    if config.debug {
        emit_log(event_tx, format!("[debug] {}", message.into()));
    }
}

async fn run_broadcast_worker(
    config: BroadcastConfig,
    mut shutdown_rx: watch::Receiver<bool>,
    event_tx: Sender<WorkerEvent>,
) -> Result<(), AnyError> {
    let mut options = ClientOptions::default();
    if let Some(path) = config.ini_path.clone() {
        options.stats_api_ini_path = Some(path);
    } else {
        options.auto_enable_packet_rate = false;
    }

    options.host = config.rl_host.clone();
    options.port_override = Some(config.rl_port);

    emit_log(
        &event_tx,
        format!(
            "Connecting to RL Stats API at {}:{}",
            config.rl_host, config.rl_port
        ),
    );

    let Some(mut client) = connect_client_with_shutdown(
        options,
        &mut shutdown_rx,
        &event_tx,
    )
    .await?
    else {
        emit_log(&event_tx, "Startup cancelled".to_string());
        return Ok(());
    };

    let source = client.connection().socket_address().to_string();
    let bind_address = format!("{}:{}", config.ws_host, config.ws_port);
    let listener = TcpListener::bind(&bind_address).await?;

    emit_log(
        &event_tx,
        format!("Connected to RL Stats API at {source}"),
    );

    let (outgoing_tx, _) = broadcast::channel::<String>(1024);
    let (accept_shutdown_tx, accept_shutdown_rx) = watch::channel(false);
    let client_count = Arc::new(AtomicUsize::new(0));

    let accept_handle = tokio::spawn(run_accept_loop(
        listener,
        outgoing_tx.clone(),
        accept_shutdown_rx,
        client_count,
        config.clone(),
        event_tx.clone(),
    ));

    let ws_url = format!("ws://{}:{}", config.ws_host, config.ws_port);
    let _ = event_tx.send(WorkerEvent::Started { source, ws_url });

    let mut relayed = 0usize;

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    emit_log(&event_tx, "Shutdown requested".to_string());
                    let _ = accept_shutdown_tx.send(true);
                    break;
                }
            }
            next_event = client.next_event() => {
                match next_event {
                    Ok(Some(event)) => {
                        for envelope in translate_stats_event(&event)? {
                            let payload = serde_json::to_string(&envelope)?;
                            let sos_event = envelope.event.clone();

                            let send_result = outgoing_tx.send(payload);
                            if let Err(error) = send_result {
                                emit_debug(
                                    &config,
                                    &event_tx,
                                    format!(
                                        "Dropped {} event (no clients): {}",
                                        sos_event,
                                        error
                                    ),
                                );
                            }

                            relayed += 1;
                            let _ = event_tx.send(WorkerEvent::Relayed { total: relayed });

                            if config.max_events.is_some_and(|max| relayed >= max) {
                                emit_log(
                                    &event_tx,
                                    format!("Max events reached ({relayed}); stopping"),
                                );
                                let _ = accept_shutdown_tx.send(true);
                                if let Err(error) = accept_handle.await {
                                    emit_log(
                                        &event_tx,
                                        format!("Accept loop join error: {error}"),
                                    );
                                }
                                return Ok(());
                            }
                        }
                    }
                    Ok(None) => {
                        emit_log(
                            &event_tx,
                            "RL event stream closed; reconnecting...".to_string(),
                        );
                        let reconnected = reconnect_rl_with_backoff(
                            &mut client,
                            &config,
                            &event_tx,
                            &mut shutdown_rx,
                        )
                        .await?;

                        if !reconnected {
                            let _ = accept_shutdown_tx.send(true);
                            break;
                        }
                    }
                    Err(error) => {
                        emit_log(
                            &event_tx,
                            format!("RL stream error: {error}; reconnecting..."),
                        );
                        let reconnected = reconnect_rl_with_backoff(
                            &mut client,
                            &config,
                            &event_tx,
                            &mut shutdown_rx,
                        )
                        .await?;

                        if !reconnected {
                            let _ = accept_shutdown_tx.send(true);
                            break;
                        }
                    }
                }
            }
        }
    }

    if let Err(error) = accept_handle.await {
        emit_log(&event_tx, format!("Accept loop join error: {error}"));
    }

    Ok(())
}

async fn run_accept_loop(
    listener: TcpListener,
    outgoing_tx: broadcast::Sender<String>,
    mut shutdown_rx: watch::Receiver<bool>,
    client_count: Arc<AtomicUsize>,
    config: BroadcastConfig,
    event_tx: Sender<WorkerEvent>,
) {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer_addr)) => {
                        emit_debug(
                            &config,
                            &event_tx,
                            format!("Incoming websocket connection from {peer_addr}"),
                        );

                        let outgoing_rx = outgoing_tx.subscribe();
                        let shutdown_child = shutdown_rx.clone();
                        let client_count_child = client_count.clone();
                        let config_child = config.clone();
                        let event_tx_child = event_tx.clone();

                        tokio::spawn(async move {
                            handle_client(
                                stream,
                                peer_addr.to_string(),
                                outgoing_rx,
                                shutdown_child,
                                client_count_child,
                                config_child,
                                event_tx_child,
                            )
                            .await;
                        });
                    }
                    Err(error) => {
                        emit_log(&event_tx, format!("Websocket accept error: {error}"));
                        sleep(Duration::from_millis(config.reconnect_ms)).await;
                    }
                }
            }
        }
    }
}

async fn handle_client(
    stream: TcpStream,
    peer: String,
    mut outgoing_rx: broadcast::Receiver<String>,
    mut shutdown_rx: watch::Receiver<bool>,
    client_count: Arc<AtomicUsize>,
    config: BroadcastConfig,
    event_tx: Sender<WorkerEvent>,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(stream) => stream,
        Err(error) => {
            emit_debug(
                &config,
                &event_tx,
                format!("Websocket handshake failed for {peer}: {error}"),
            );
            return;
        }
    };

    let connected = client_count.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = event_tx.send(WorkerEvent::Clients(connected));
    emit_log(
        &event_tx,
        format!("Overlay client connected ({peer}); clients={connected}"),
    );

    let (mut writer, mut reader) = ws_stream.split();

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            outgoing = outgoing_rx.recv() => {
                match outgoing {
                    Ok(payload) => {
                        if let Err(error) = writer.send(Message::Text(payload.into())).await {
                            emit_debug(
                                &config,
                                &event_tx,
                                format!("Send error for {peer}: {error}"),
                            );
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        emit_debug(
                            &config,
                            &event_tx,
                            format!(
                                "Client {peer} lagged and skipped {skipped} payload(s)"
                            ),
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            inbound = reader.next() => {
                match inbound {
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(error) = writer.send(Message::Pong(payload)).await {
                            emit_debug(
                                &config,
                                &event_tx,
                                format!("Failed pong to {peer}: {error}"),
                            );
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        emit_debug(
                            &config,
                            &event_tx,
                            format!("Client {peer} receive error: {error}"),
                        );
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    let _ = writer.close().await;

    let remaining = client_count
        .fetch_sub(1, Ordering::SeqCst)
        .saturating_sub(1);
    let _ = event_tx.send(WorkerEvent::Clients(remaining));
    emit_log(
        &event_tx,
        format!("Overlay client disconnected ({peer}); clients={remaining}"),
    );
}

async fn reconnect_rl_with_backoff(
    client: &mut RocketLeagueStatsClient,
    config: &BroadcastConfig,
    event_tx: &Sender<WorkerEvent>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<bool, AnyError> {
    let mut last_error: Option<AnyError> = None;

    for attempt in 1..=10 {
        if *shutdown_rx.borrow() {
            emit_log(event_tx, "Shutdown requested".to_string());
            return Ok(false);
        }

        emit_log(
            event_tx,
            format!("Reconnecting to RL Stats API (attempt {attempt}/10)..."),
        );

        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    emit_log(event_tx, "Shutdown requested".to_string());
                    return Ok(false);
                }
            }
            reconnect_result = timeout(Duration::from_secs(3), client.reconnect()) => {
                match reconnect_result {
                    Ok(Ok(())) => {
                        emit_log(
                            event_tx,
                            format!(
                                "Reconnected to RL Stats API at {}",
                                client.connection().socket_address()
                            ),
                        );
                        return Ok(true);
                    }
                    Ok(Err(error)) => {
                        emit_debug(
                            config,
                            event_tx,
                            format!(
                                "Reconnect failed on attempt {attempt}/10: {error}"
                            ),
                        );
                        last_error = Some(Box::new(error));
                    }
                    Err(_) => {
                        let timeout_error = std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "reconnect attempt timed out",
                        );
                        emit_debug(
                            config,
                            event_tx,
                            format!(
                                "Reconnect failed on attempt {attempt}/10: {timeout_error}"
                            ),
                        );
                        last_error = Some(Box::new(timeout_error));
                    }
                }
            }
        }

        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    emit_log(event_tx, "Shutdown requested".to_string());
                    return Ok(false);
                }
            }
            _ = sleep(Duration::from_millis(config.reconnect_ms)) => {}
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => Err("Reconnect failed".into()),
    }
}

async fn connect_client_with_shutdown(
    options: ClientOptions,
    shutdown_rx: &mut watch::Receiver<bool>,
    event_tx: &Sender<WorkerEvent>,
) -> Result<Option<RocketLeagueStatsClient>, AnyError> {
    let mut last_error: Option<AnyError> = None;

    for attempt in 1..=20 {
        if *shutdown_rx.borrow() {
            return Ok(None);
        }

        if attempt > 1 {
            emit_log(
                event_tx,
                format!(
                    "Retrying RL Stats API connection (attempt {attempt}/20)..."
                ),
            );
        }

        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    return Ok(None);
                }
            }
            connect_result = timeout(Duration::from_secs(3), RocketLeagueStatsClient::connect(options.clone())) => {
                match connect_result {
                    Ok(Ok(client)) => return Ok(Some(client)),
                    Ok(Err(error)) => {
                        emit_log(
                            event_tx,
                            format!(
                                "Initial connection failed on attempt {attempt}/20: {error}"
                            ),
                        );
                        last_error = Some(Box::new(error));
                    }
                    Err(_) => {
                        let timeout_error = std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "initial connection attempt timed out",
                        );
                        last_error = Some(Box::new(timeout_error));
                    }
                }
            }
        }

        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    return Ok(None);
                }
            }
            _ = sleep(Duration::from_millis(250)) => {}
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => Err("Failed to connect to RL Stats API".into()),
    }
}

fn main() -> Result<(), eframe::Error> {
    configure_linux_display_backend();

    let options = eframe::NativeOptions {
        persist_window: false,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(DEFAULT_WINDOW_SIZE)
            .with_min_inner_size(MIN_WINDOW_SIZE)
            .with_title("SOS Broadcaster"),
        ..Default::default()
    };

    eframe::run_native(
        "SOS Broadcaster",
        options,
        Box::new(|cc| Ok(Box::new(SosBroadcastGuiApp::new(cc)))),
    )
}

#[cfg(target_os = "linux")]
fn configure_linux_display_backend() {
    if !is_wsl() {
        return;
    }

    // In winit 0.30, backend selection prioritizes WAYLAND_DISPLAY/
    // WAYLAND_SOCKET over DISPLAY. On many WSL setups that leads to
    // Wayland selection even when no usable compositor is present.
    // If DISPLAY is available, force X11 by clearing Wayland selectors.
    if has_display_env() {
        let had_wayland_env = has_wayland_env();

        // SAFETY: this runs before the GUI event loop and before any threads are
        // started in this process, so mutating process env here is safe.
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::remove_var("WAYLAND_SOCKET");
        }
        if had_wayland_env {
            eprintln!(
                "[info] WSL detected; forcing X11 by ignoring WAYLAND_* env vars"
            );
        } else {
            eprintln!("[info] WSL detected; using X11 backend");
        }
    } else {
        eprintln!(
            "[warn] WSL detected but DISPLAY is not set. Start an X server/WSLg or run the CLI broadcaster instead."
        );
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_display_backend() {}

#[cfg(target_os = "linux")]
fn has_display_env() -> bool {
    std::env::var("DISPLAY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn has_wayland_env() -> bool {
    std::env::var("WAYLAND_DISPLAY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        || std::env::var("WAYLAND_SOCKET")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn is_wsl() -> bool {
    if let Ok(value) = std::env::var("WSL_DISTRO_NAME") {
        if !value.trim().is_empty() {
            return true;
        }
    }

    if let Ok(version) = std::fs::read_to_string("/proc/version") {
        let lowered = version.to_ascii_lowercase();
        return lowered.contains("microsoft") || lowered.contains("wsl");
    }

    false
}
