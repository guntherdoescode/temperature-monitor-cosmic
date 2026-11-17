use crate::sensors::{self, SensorKind, SensorReading, Snapshot, ThermalState};
use cosmic::{iced::{Alignment, Length, Subscription, futures, stream}, prelude::*, theme, widget};
use futures::SinkExt;
use std::{collections::HashMap, time::{Duration, Instant}};

const APP_ID: &str = "io.github.guntherdoescode.CosmicTemperatureMonitor";
const REFRESH_INTERVAL: Duration = Duration::from_millis(800);

pub struct AppModel {
    core: cosmic::Core,
    readings: Vec<SensorReading>,
    history: HashMap<String, Vec<f64>>,
    throttle_total: Option<u64>,
    throttle_event: bool,
    last_updated: Option<Instant>,
    error: Option<String>,
    refreshing: bool,
}

#[derive(Debug, Clone)]
pub enum Message { Refresh, SensorsLoaded(Result<Snapshot, String>) }

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::Core { &self.core }
    fn core_mut(&mut self) -> &mut cosmic::Core { &mut self.core }

    fn init(core: cosmic::Core, _flags: Self::Flags) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let app = Self {
            core, readings: Vec::new(), history: HashMap::new(),
            throttle_total: None, throttle_event: false, last_updated: None,
            error: None, refreshing: true,
        };
        let task = app.refresh_task();
        (app, task)
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let title = widget::column::with_capacity(2).spacing(4)
            .push(widget::text::title1("COSMIC Temperature Monitor"))
            .push(widget::text::body("THERMAL INTELLIGENCE  /  LOCAL SYSTEM"))
            .into();

        let mut content = widget::column::with_capacity(8).spacing(18).padding(24).width(Length::Fill);
        content = content.push(widget::row::with_children([title]).align_y(Alignment::Center).spacing(12));

        let hot_count = self.readings.iter().filter(|r| r.state == ThermalState::Hot).count();
        let (summary, tint) = if self.throttle_event {
            ("CPU THROTTLING OBSERVED IN THIS SAMPLE", color(255, 92, 98))
        } else if hot_count > 0 {
            ("ELEVATED TEMPERATURE — KEEP AN EYE ON IT", color(255, 92, 98))
        } else if self.readings.is_empty() {
            ("WAITING FOR SENSOR DATA", color(101, 168, 255))
        } else {
            ("ALL REPORTED SENSORS WITHIN THEIR CURRENT LIMITS", color(101, 168, 255))
        };

        content = content.push(widget::container(
            widget::column::with_capacity(3).spacing(8)
                .push(widget::text::body("SYSTEM STATUS"))
                .push(widget::text::title3(summary).class(theme::Text::Color(tint)))
                .push(widget::text::body(self.updated_label()))
                .push(widget::text::body(if self.throttle_total.is_some() { "THROTTLE TELEMETRY: AVAILABLE" } else { "THROTTLE TELEMETRY: NOT EXPOSED BY THIS KERNEL / DRIVER" })),
        ).width(Length::Fill).padding(18).class(theme::Container::Card));

        if let Some(error) = &self.error {
            content = content.push(widget::container(
                widget::text::body(format!("Sensor error: {error}")).class(theme::Text::Color(color(255, 117, 117))),
            ).width(Length::Fill).padding(14).class(theme::Container::Card));
        }

        let legend = widget::row::with_children([
            legend_item("COOL", color(72, 215, 242)),
            legend_item("OPERATIONAL", color(101, 168, 255)),
            legend_item("HOT / THROTTLING", color(255, 92, 98)),
        ]).spacing(14).align_y(Alignment::Center);

        content = content.push(widget::row::with_children([
            widget::text::heading("SENSOR ARRAY").into(),
            widget::space::horizontal().into(),
            legend.into(),
        ]).align_y(Alignment::Center));

        if self.readings.is_empty() && self.error.is_none() {
            content = content.push(widget::container(
                widget::column::with_capacity(2).spacing(8)
                    .push(widget::text::title3("No readings yet"))
                    .push(widget::text::body("COSMIC Temperature Monitor is querying lm-sensors. Install lm-sensors and check that the sensors command returns readings.")),
            ).width(Length::Fill).padding(20).class(theme::Container::Card));
        } else {
            for pair in self.readings.chunks(2) {
                let mut row = widget::row::with_capacity(2).spacing(14);
                for reading in pair { row = row.push(self.sensor_card(reading)); }
                if pair.len() == 1 { row = row.push(widget::space::horizontal().width(Length::Fill)); }
                content = content.push(row);
            }
        }

        content = content.push(widget::text::body(
            "SOURCE: lm-sensors  •  POLL: 0.8s  •  THROTTLE: Linux thermal_throttle counters",
        ));
        widget::container(widget::scrollable(content)).width(Length::Fill).height(Length::Fill).into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::run(|| stream::channel(1, |mut sender: futures::channel::mpsc::Sender<Message>| async move {
            let mut interval = tokio::time::interval(REFRESH_INTERVAL);
            loop {
                interval.tick().await;
                let _ = sender.send(Message::Refresh).await;
            }
        }))
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::Refresh => {
                if self.refreshing { return Task::none(); }
                self.refreshing = true;
                self.refresh_task()
            }
            Message::SensorsLoaded(Ok(snapshot)) => {
                self.refreshing = false;
                self.error = None;
                self.throttle_total = snapshot.throttle_total;
                self.throttle_event = snapshot.throttle_event;
                for reading in &snapshot.readings {
                    let values = self.history.entry(reading.id.clone()).or_default();
                    values.push(reading.temperature);
                    if values.len() > 32 { values.remove(0); }
                }
                self.readings = snapshot.readings;
                self.last_updated = Some(Instant::now());
                Task::none()
            }
            Message::SensorsLoaded(Err(error)) => {
                self.refreshing = false;
                self.error = Some(error);
                self.last_updated = Some(Instant::now());
                Task::none()
            }
        }
    }
}

impl AppModel {
    fn refresh_task(&self) -> Task<cosmic::Action<Message>> {
        let previous = self.throttle_total;
        Task::perform(async move {
            tokio::task::spawn_blocking(move || sensors::collect(previous)).await
                .unwrap_or_else(|error| Err(format!("Sensor worker failed: {error}")))
        }, Message::SensorsLoaded).map(cosmic::Action::App)
    }

    fn updated_label(&self) -> String {
        self.last_updated.map(|instant| format!("Last scan {:.1}s ago", instant.elapsed().as_secs_f64()))
            .unwrap_or_else(|| "Initial scan in progress".to_owned())
    }

    fn sensor_card<'a>(&'a self, reading: &'a SensorReading) -> Element<'a, Message> {
        let (state_label, accent) = match reading.state {
            ThermalState::Cool => ("COOL", color(72, 215, 242)),
            ThermalState::Normal => ("OPERATIONAL", color(101, 168, 255)),
            ThermalState::Hot => ("HOT", color(255, 92, 98)),
        };
        let identity = widget::column::with_capacity(2).spacing(0)
            .push(widget::text::caption(reading.chip.clone()))
            .push(widget::text::heading(reading.label.clone()));
        let mut heading = widget::row::with_capacity(4)
            .push(sensor_icon(reading.kind, accent))
            .push(identity)
            .push(widget::space::horizontal())
            .push(widget::text::caption(state_label).class(theme::Text::Color(accent)))
            .align_y(Alignment::Center)
            .spacing(8);
        if reading.throttled {
            heading = heading.push(widget::text::caption("THROTTLING").class(theme::Text::Color(color(255, 92, 98))));
        }

        let history = self.history.get(&reading.id).map(|values| sparkline(values))
            .unwrap_or_else(|| "Collecting history…".to_owned());
        let limits = format!("WARN {:.0}°C    CRIT {:.0}°C", reading.warning, reading.critical);
        let temperature = widget::row::with_capacity(2).spacing(8)
            .push(widget::text::title3(format!("{:.1}°C", reading.temperature)).class(theme::Text::Color(accent)))
            .push(widget::text::caption(limits))
            .align_y(Alignment::Center);
        let card = widget::column::with_capacity(4).spacing(6)
            .push(heading)
            .push(temperature)
            .push(widget::text::caption(history).class(theme::Text::Color(accent)));
        widget::container(card).width(Length::Fill).padding(12).class(sensor_card_style()).into()
    }
}

fn sensor_card_style() -> theme::Container<'static> {
    theme::Container::custom(|theme| {
        let cosmic = theme.cosmic();
        let component = &cosmic.background(false).component;
        widget::container::Style {
            background: Some(cosmic::iced::Background::Color(component.base.into())),
            border: cosmic::iced::Border { radius: 12.0.into(), ..Default::default() },
            text_color: Some(component.on.into()),
            icon_color: Some(component.on.into()),
            ..Default::default()
        }
    })
}

fn sensor_icon<'a>(kind: SensorKind, tint: cosmic::iced::Color) -> Element<'a, Message> {
    let bytes: &'static [u8] = match kind {
        SensorKind::Cpu => include_bytes!("../resources/icons/sensors/cpu.svg"),
        SensorKind::Gpu => include_bytes!("../resources/icons/sensors/gpu.svg"),
        SensorKind::Storage => include_bytes!("../resources/icons/sensors/storage.svg"),
        SensorKind::Board => include_bytes!("../resources/icons/sensors/board.svg"),
        SensorKind::Fan => include_bytes!("../resources/icons/sensors/fan.svg"),
        SensorKind::Battery => include_bytes!("../resources/icons/sensors/battery.svg"),
        SensorKind::Memory => include_bytes!("../resources/icons/sensors/memory.svg"),
        SensorKind::Network => include_bytes!("../resources/icons/sensors/network.svg"),
        SensorKind::Generic => include_bytes!("../resources/icons/sensors/thermometer.svg"),
    };
    widget::icon::icon(widget::icon::from_svg_bytes(bytes))
        .class(theme::Svg::custom(move |_theme| widget::svg::Style { color: Some(tint) }))
        .size(22)
        .into()
}

fn legend_item(label: &'static str, tint: cosmic::iced::Color) -> Element<'static, Message> {
    widget::text::body(label).class(theme::Text::Color(tint)).into()
}
fn color(red: u8, green: u8, blue: u8) -> cosmic::iced::Color {
    cosmic::iced::Color::from_rgb8(red, green, blue)
}
fn sparkline(values: &[f64]) -> String {
    const BARS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    if values.is_empty() { return "Collecting history…".to_owned(); }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).max(0.5);
    values.iter().map(|value| {
        let index = (((value - min) / span) * 7.0).round() as usize;
        BARS[index.min(7)]
    }).collect()
}
