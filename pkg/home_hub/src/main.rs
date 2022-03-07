extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate peripheral;
extern crate rpi;
extern crate stream_deck;
#[macro_use]
extern crate macros;
extern crate container;
extern crate home_hub;
extern crate hue;
extern crate protobuf;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use common::async_std::channel;
use common::async_std::fs;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::{errors::*, project_path};
use home_hub::proto::config::Config;
use peripheral::ddc::DDCDevice;
use rpi::gpio::*;
use rpi::pwm::*;
use stream_deck::StreamDeckDevice;

#[derive(Args)]
struct Args {
    hdmi_ddc_device: String,
    config_object: String,
}

#[derive(Debug)]
pub enum Event {
    KeyUp(usize),
    KeyDown(usize),
}

const INPUT_SELECT_VCP_CODE: u8 = 0x60;

enum_def_with_unknown!(InputSelectValue u8 =>
    AnalogVideo1 = 0x01, // RGB 1
    AnalogVideo2 = 0x02, // RGB 2
    DigitalVideo1 = 0x03, // DVI 1
    DigitalVideo2 = 0x04, // DVI 2
    CompositeVideo1 = 0x05,
    CompositeVideo2 = 0x06,
    SVideo1 = 0x07,
    SVideo2 = 0x08,
    Tuner1 = 0x09,
    Tuner2 = 0x0A,
    Tuner3 = 0x0B,
    ComponentVideo1 = 0x0C,
    ComponentVideo2 = 0x0D,
    ComponentVideo3 = 0x0E,
    DisplayPort1 = 0x0F,
    DisplayPort2 = 0x10,
    HDMI1 = 0x11, // Digital Video 3
    HDMI2 = 0x12 // Digital Video 4
);

struct App {
    state: Mutex<State>,
    state_event: (channel::Sender<()>, channel::Receiver<()>),
    light_event: (channel::Sender<()>, channel::Receiver<()>),
    ddc_event: (channel::Sender<()>, channel::Receiver<()>),
}

#[derive(Clone, PartialEq, Default)]
struct State {
    /// This will be None if it's unknown.
    active_display_input: Option<InputSelectValue>,
    pending_display_input: Option<InputSelectValue>,

    entry_light_on: Option<bool>,
    pending_entry_light_on: Option<bool>,

    study_light_on: Option<bool>,
    pending_study_light_on: Option<bool>,
}

impl App {
    pub async fn run() -> Result<()> {
        let args = common::args::parse_args::<Args>()?;

        let mut meta_client =
            container::meta::client::ClusterMetaClient::create_from_environment().await?;

        let config = meta_client
            .get_object::<Config>(&args.config_object)
            .await?
            .ok_or_else(|| err_msg("No config found in cluster"))?;

        let deck = StreamDeckDevice::open().await?;
        deck.set_display_timeout(60).await?;

        let ddc = DDCDevice::open(&args.hdmi_ddc_device)?;

        let hue_client = {
            let client = hue::AnonymousHueClient::create().await?;
            hue::HueClient::create(client, config.hue_user_name())
        };

        let inst = Arc::new(Self {
            state: Mutex::new(State::default()),
            state_event: channel::bounded(1),
            light_event: channel::bounded(1),
            ddc_event: channel::bounded(1),
        });

        // Enqueue first round of updates on each thread.
        inst.state_event.0.send(()).await?;
        inst.light_event.0.send(()).await?;
        inst.ddc_event.0.send(()).await?;

        let mut task_bundle = common::bundle::TaskResultBundle::new();
        task_bundle.add("App::render_thread", inst.clone().render_thread(deck));
        task_bundle.add("App::light_thread", inst.clone().light_thread(hue_client));
        task_bundle.add("App::ddc_thread", inst.clone().ddc_thread(ddc));
        task_bundle.join().await
    }

    /// Performs Stream Deck rendering and event listening.
    async fn render_thread(self: Arc<Self>, deck: StreamDeckDevice) -> Result<()> {
        const DISPLAY_COMPUTER_BUTTON: usize = 0;
        let computer_active =
            fs::read(project_path!("pkg/home_hub/icons/computer-active.jpg")).await?;
        let computer_default = fs::read(project_path!("pkg/home_hub/icons/computer.jpg")).await?;

        const DISPLAY_LAPTOP_BUTTON: usize = 1;
        let laptop_active = fs::read(project_path!("pkg/home_hub/icons/laptop-active.jpg")).await?;
        let laptop_default = fs::read(project_path!("pkg/home_hub/icons/laptop.jpg")).await?;

        const LIGHT_ENTRY_BUTTON: usize = 5;
        let light_on_entry =
            fs::read(project_path!("pkg/home_hub/icons/light-on-entry.jpg")).await?;
        let light_off_entry =
            fs::read(project_path!("pkg/home_hub/icons/light-off-entry.jpg")).await?;

        const LIGHT_STUDY_BUTTON: usize = 6;
        let light_on_study =
            fs::read(project_path!("pkg/home_hub/icons/light-on-study.jpg")).await?;
        let light_off_study =
            fs::read(project_path!("pkg/home_hub/icons/light-off-study.jpg")).await?;

        let error_jpg = fs::read(project_path!("pkg/home_hub/icons/error.jpg")).await?;

        let mut last_key_state = vec![];

        let mut last_view_state = State::default();
        let mut first_render = true;

        enum FutureEvent {
            StateChange,
            KeyState(Result<Vec<stream_deck::KeyState>>),
        }

        loop {
            let event = common::async_std::future::timeout(
                Duration::from_secs(10),
                common::future::race(
                    common::future::map(self.state_event.1.recv(), |_| FutureEvent::StateChange),
                    // TODO: Need a better way to cancel this if the state change happens first
                    // (such that we don't miss events if they are received
                    // during future cancellation).
                    common::future::map(Box::pin(deck.poll_key_state()), |e| {
                        FutureEvent::KeyState(e)
                    }),
                ),
            )
            .await;

            match event {
                Err(_) => {
                    // Timeout. Mainly to keep things alive in case of bugs.
                }
                Ok(FutureEvent::StateChange) => {
                    // Get the current state and exit early if the value hasn't actually changed.
                    let mut current_state = self.state.lock().await.clone();
                    if !first_render && current_state == last_view_state {
                        continue;
                    }
                    first_render = false;
                    last_view_state = current_state.clone();

                    deck.set_key_image(
                        DISPLAY_COMPUTER_BUTTON,
                        match current_state.active_display_input.clone() {
                            Some(InputSelectValue::DisplayPort1) => &computer_active,
                            Some(_) => &computer_default,
                            None => &error_jpg,
                        },
                    )
                    .await?;

                    deck.set_key_image(
                        DISPLAY_LAPTOP_BUTTON,
                        match current_state.active_display_input.clone() {
                            Some(InputSelectValue::HDMI2) => &laptop_active,
                            Some(_) => &laptop_default,
                            None => &error_jpg,
                        },
                    )
                    .await?;

                    deck.set_key_image(
                        LIGHT_ENTRY_BUTTON,
                        match current_state.entry_light_on.clone() {
                            Some(true) => &light_on_entry,
                            Some(false) => &light_off_entry,
                            None => &error_jpg,
                        },
                    )
                    .await?;

                    deck.set_key_image(
                        LIGHT_STUDY_BUTTON,
                        match current_state.study_light_on.clone() {
                            Some(true) => &light_on_study,
                            Some(false) => &light_off_study,
                            None => &error_jpg,
                        },
                    )
                    .await?;
                }
                Ok(FutureEvent::KeyState(key_state)) => {
                    let key_state = key_state?;

                    println!("GOT EVENTS");
                    let mut events = vec![];
                    for i in 0..key_state.len() {
                        let old_value = last_key_state
                            .get(i)
                            .map(|v| *v)
                            .unwrap_or(stream_deck::KeyState::Up);

                        if old_value == key_state[i] {
                            continue;
                        } else if key_state[i] == stream_deck::KeyState::Up {
                            events.push(Event::KeyUp(i));
                        } else {
                            events.push(Event::KeyDown(i));
                        }
                    }
                    last_key_state = key_state;

                    let mut state = self.state.lock().await;

                    for event in events {
                        println!("{:?}", event);
                        match event {
                            Event::KeyDown(DISPLAY_COMPUTER_BUTTON) => {
                                state.pending_display_input = Some(InputSelectValue::DisplayPort1);
                                let _ = self.ddc_event.0.try_send(());
                            }
                            Event::KeyDown(DISPLAY_LAPTOP_BUTTON) => {
                                state.pending_display_input = Some(InputSelectValue::HDMI2);
                                let _ = self.ddc_event.0.try_send(());
                            }
                            Event::KeyDown(LIGHT_ENTRY_BUTTON) => {
                                state.pending_entry_light_on =
                                    Some(!state.entry_light_on.unwrap_or(false));
                                let _ = self.light_event.0.try_send(());
                            }
                            Event::KeyDown(LIGHT_STUDY_BUTTON) => {
                                state.pending_study_light_on =
                                    Some(!state.study_light_on.unwrap_or(false));
                                let _ = self.light_event.0.try_send(());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    async fn ddc_thread(self: Arc<Self>, mut ddc: DDCDevice) -> Result<()> {
        loop {
            // Wait for either a timeout or DDC event.
            let _ = common::async_std::future::timeout(
                Duration::from_secs(10),
                self.ddc_event.1.recv(),
            )
            .await;

            let pending_input = {
                let mut state = self.state.lock().await;
                state.pending_display_input.take()
            };

            if let Some(input) = pending_input {
                ddc.set_vcp_feature(INPUT_SELECT_VCP_CODE, input.to_value() as u16)?;
            }

            // Timeout: Check again what the display thinks the current
            // input is.

            let mut num_attempts = 0;

            let feature;
            loop {
                match ddc.get_vcp_feature(INPUT_SELECT_VCP_CODE) {
                    Ok(f) => {
                        feature = f;
                        break;
                    }
                    Err(e) => {
                        num_attempts += 1;
                        if num_attempts == 60 {
                            return Err(e);
                        }

                        eprintln!("Failure getting feature (attempt {}): {}", num_attempts, e);

                        // TODO: Exponential backoff.
                        common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }

            let current_value = InputSelectValue::from_value((feature.current_value & 0xff) as u8);

            {
                let mut state = self.state.lock().await;
                state.active_display_input = Some(current_value);
                let _ = self.state_event.0.try_send(());
            }
        }
    }

    async fn light_thread(self: Arc<Self>, client: hue::HueClient) -> Result<()> {
        loop {
            let _ = common::async_std::future::timeout(
                Duration::from_secs(10),
                self.light_event.1.recv(),
            )
            .await;

            let groups;
            // TODO: Improve this as this is dangerous as is:
            // 1. implement this as retrying in the http handler.
            // 2. Use a fresh connection to perform retries (a connection that has had no
            // traffic in a while may have timed out).
            let mut attempt = 0;
            loop {
                attempt += 1;
                match client.get_groups().await {
                    Ok(v) => {
                        groups = v;
                        break;
                    }
                    Err(e) => {
                        println!("{:?}", e);
                        if attempt >= 5 {
                            return Err(err_msg("Exceeded max attempts to get_groups"));
                        }

                        continue;
                    }
                }
            }

            let (pending_entry, pending_study) = {
                let mut state = self.state.lock().await;
                (
                    state.pending_entry_light_on.take(),
                    state.pending_study_light_on.take(),
                )
            };

            let mut entry_light_on = None;
            let mut study_light_on = None;

            for (group_id, group) in groups {
                if group.name == "Entry" {
                    entry_light_on = Some(group.all_on);
                    if let Some(value) = pending_entry {
                        client.set_group_on(&group_id, value).await?;
                        entry_light_on = Some(value);
                    }
                } else if group.name == "Study" {
                    study_light_on = Some(group.all_on);
                    if let Some(value) = pending_study {
                        client.set_group_on(&group_id, value).await?;
                        study_light_on = Some(value);
                    }
                }
            }

            let mut state = self.state.lock().await;
            state.entry_light_on = entry_light_on;
            state.study_light_on = study_light_on;
            let _ = self.state_event.0.try_send(());
        }
    }
}

fn main() -> Result<()> {
    task::block_on(App::run())
}
