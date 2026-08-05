use crate::{
    command::RuntimeCommand,
    config::ConfigManager,
    error::AppResult,
    media::MediaWatcher,
    model::AppConfig,
    osc::{FrameMode, OscSender, WriteEvent},
    oscquery::{OscEndpoint, OscQueryClient},
    text::TextComposer,
};

use std::time::{Duration, Instant};

use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    time::{Instant as TokioInstant, Interval, MissedTickBehavior, interval_at},
};

const CONFIG_CHECK_INTERVAL: Duration = Duration::from_secs(1);

const DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

const MIN_WRITE_STEP_MS: u64 = 20;

#[derive(Default)]
struct ConfigChange {
    write_timer_changed: bool,
    discovery_changed: bool,
    synchronization_reset: bool,
}

struct DiscoveryMessage {
    generation: u64,
    result: AppResult<Option<OscEndpoint>>,
}

pub async fn run(mut commands: UnboundedReceiver<RuntimeCommand>) -> AppResult<()> {
    let mut config = ConfigManager::load()?;

    let mut config_modified = ConfigManager::modified_time()?;

    let media = MediaWatcher::start();

    let mut composer = create_composer(&config);

    let mut osc = OscSender::new(&config.osc, &config.parameters, composer.buffer_length())?;

    let mut active_target = (config.osc.host.clone(), config.osc.port);

    let mut write_timer = make_write_timer(config.text.write_step_ms);

    let mut config_timer = make_timer(CONFIG_CHECK_INTERVAL, MissedTickBehavior::Skip);

    let mut discovery_timer = make_timer(DISCOVERY_INTERVAL, MissedTickBehavior::Skip);

    let (discovery_sender, mut discovery_receiver) = mpsc::unbounded_channel::<DiscoveryMessage>();

    let mut discovery_generation = 0_u64;

    let mut active_discovery = None::<u64>;

    let mut full_refresh_pending = true;

    let mut last_full_refresh_completed = Instant::now();

    request_discovery(
        &config,
        &discovery_sender,
        &mut discovery_generation,
        &mut active_discovery,
    );

    tracing::info!(
        write_step_ms = config.text.write_step_ms,
        scroll_interval_ms = config.text.scroll_interval_ms,
        full_refresh_seconds = config.text.full_refresh_seconds,
        slots = composer.buffer_length(),
        "Backend runtime started"
    );

    loop {
        tokio::select! {
            biased;

            command = commands.recv() => {
                match command {
                    Some(
                        RuntimeCommand::ReloadConfig,
                    ) => {
                        let next =
                            match ConfigManager::load() {
                                Ok(config) => config,

                                Err(error) => {
                                    tracing::warn!(
                                        error = %error,
                                        "Could not reload configuration"
                                    );

                                    continue;
                                }
                            };

                        match ConfigManager::modified_time() {
                            Ok(modified) => {
                                config_modified =
                                    modified;
                            }

                            Err(error) => {
                                tracing::debug!(
                                    error = %error,
                                    "Could not read configuration modification time"
                                );
                            }
                        }

                        match apply_config(
                            next,
                            &mut config,
                            &mut composer,
                            &mut osc,
                            &mut active_target,
                        ) {
                            Ok(change) => {
                                if change
                                    .write_timer_changed
                                {
                                    write_timer =
                                        make_write_timer(
                                            config
                                                .text
                                                .write_step_ms,
                                        );
                                }

                                if change
                                    .synchronization_reset
                                {
                                    full_refresh_pending =
                                        true;
                                }

                                if change
                                    .discovery_changed
                                {
                                    request_discovery(
                                        &config,
                                        &discovery_sender,
                                        &mut discovery_generation,
                                        &mut active_discovery,
                                    );
                                }

                                tracing::info!(
                                    "Configuration reloaded"
                                );
                            }

                            Err(error) => {
                                tracing::warn!(
                                    error = %error,
                                    "Could not apply reloaded configuration"
                                );
                            }
                        }
                    }

                    Some(
                        RuntimeCommand::Shutdown,
                    )
                    | None => {
                        break;
                    }
                }
            }

            message =
                discovery_receiver.recv()
            => {
                let Some(message) =
                    message
                else {
                    continue;
                };

                if active_discovery
                    != Some(
                        message.generation,
                    )
                {
                    continue;
                }

                active_discovery = None;

                if !config
                    .osc
                    .auto_discover
                    || !config
                        .oscquery
                        .enabled
                {
                    continue;
                }

                match message.result {
                    Ok(Some(endpoint)) => {
                        if apply_endpoint(
                            endpoint,
                            &mut osc,
                            &mut active_target,
                        ) {
                            full_refresh_pending =
                                true;
                        }
                    }

                    Ok(None) => {
                        tracing::debug!(
                            "VRChat was not discovered through OSCQuery"
                        );
                    }

                    Err(error) => {
                        tracing::debug!(
                            error = %error,
                            "OSCQuery discovery failed"
                        );
                    }
                }
            }

            _ = config_timer.tick() => {
                match ConfigManager::load_if_changed(
                    &mut config_modified,
                ) {
                    Ok(Some(next)) => {
                        match apply_config(
                            next,
                            &mut config,
                            &mut composer,
                            &mut osc,
                            &mut active_target,
                        ) {
                            Ok(change) => {
                                if change
                                    .write_timer_changed
                                {
                                    write_timer =
                                        make_write_timer(
                                            config
                                                .text
                                                .write_step_ms,
                                        );
                                }

                                if change
                                    .synchronization_reset
                                {
                                    full_refresh_pending =
                                        true;
                                }

                                if change
                                    .discovery_changed
                                {
                                    request_discovery(
                                        &config,
                                        &discovery_sender,
                                        &mut discovery_generation,
                                        &mut active_discovery,
                                    );
                                }
                            }

                            Err(error) => {
                                tracing::warn!(
                                    error = %error,
                                    "Could not apply changed configuration"
                                );
                            }
                        }
                    }

                    Ok(None) => {}

                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "Could not check configuration file"
                        );
                    }
                }
            }

            _ = discovery_timer.tick() => {
                if active_discovery.is_none() {
                    request_discovery(
                        &config,
                        &discovery_sender,
                        &mut discovery_generation,
                        &mut active_discovery,
                    );
                }
            }

            _ = write_timer.tick() => {
                let refresh_interval =
                    Duration::from_secs(
                        config
                            .text
                            .full_refresh_seconds
                            .max(5),
                    );

                if !full_refresh_pending
                    && last_full_refresh_completed
                        .elapsed()
                        >= refresh_interval
                {
                    full_refresh_pending =
                        true;

                    tracing::debug!(
                        "A full display refresh is pending"
                    );
                }

                if osc.is_idle() {
                    let media_state =
                        media.borrow().clone();

                    let frame =
                        composer.compose(
                            &media_state,
                            &config
                                .text
                                .separator,
                            Instant::now(),
                        );

                    let mode =
                        if full_refresh_pending {
                            FrameMode::Full
                        } else {
                            FrameMode::Delta
                        };

                    match osc.begin_frame(
                        &frame.characters,
                        mode,
                    ) {
                        Ok(true) => {
                            tracing::trace!(
                                mode = ?mode,

                                title_offset =
                                    frame.title_offset,

                                pending =
                                    osc.pending_count(),

                                "Frozen display frame started"
                            );
                        }

                        Ok(false) => {}

                        Err(error) => {
                            tracing::warn!(
                                error = %error,
                                "Could not start a display frame"
                            );

                            continue;
                        }
                    }
                }

                match osc.tick() {
                    Ok(
                        WriteEvent::Idle,
                    ) => {}

                    Ok(
                        WriteEvent::PointerPrimed,
                    ) => {
                        tracing::trace!(
                            "OSC pointer primed"
                        );
                    }

                    Ok(
                        WriteEvent::CharacterSent {
                            slot,
                            character,
                        },
                    ) => {
                        tracing::trace!(
                            slot,
                            character,
                            "OSC character sent"
                        );
                    }

                    Ok(
                        WriteEvent::SlotCommitted {
                            slot,
                            character,
                        },
                    ) => {
                        tracing::trace!(
                            slot,
                            character,

                            pending =
                                osc.pending_count(),

                            "OSC slot committed"
                        );
                    }

                    Ok(
                        WriteEvent::FrameCompleted {
                            slot,
                            character,
                            mode,
                        },
                    ) => {
                        if mode
                            == FrameMode::Full
                        {
                            full_refresh_pending =
                                false;

                            last_full_refresh_completed =
                                Instant::now();

                            tracing::debug!(
                                "Full display refresh completed"
                            );
                        }

                        tracing::trace!(
                            slot,
                            character,
                            mode = ?mode,
                            "Frozen display frame completed"
                        );
                    }

                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "Could not advance the OSC writer"
                        );
                    }
                }
            }
        }
    }

    tracing::info!("Backend runtime stopped");

    Ok(())
}

fn apply_config(
    next: AppConfig,
    current: &mut AppConfig,
    composer: &mut TextComposer,
    osc: &mut OscSender,
    active_target: &mut (String, u16),
) -> AppResult<ConfigChange> {
    if *current == next {
        return Ok(ConfigChange::default());
    }

    let write_timer_changed = current.text.write_step_ms != next.text.write_step_ms;

    let text_changed = current.text != next.text;

    let parameters_changed = current.parameters != next.parameters;

    let configured_target_changed =
        current.osc.host != next.osc.host || current.osc.port != next.osc.port;

    let automatic_discovery_disabled = current.osc.auto_discover && !next.osc.auto_discover;

    let discovery_changed = current.osc != next.osc || current.oscquery != next.oscquery;

    let mut synchronization_reset = false;

    if configured_target_changed || automatic_discovery_disabled {
        if osc.set_target(&next.osc.host, next.osc.port)? {
            synchronization_reset = true;
        }

        *active_target = (next.osc.host.clone(), next.osc.port);
    }

    if parameters_changed && osc.set_parameters(&next.parameters)? {
        synchronization_reset = true;
    }

    if text_changed {
        composer.reconfigure(
            next.text.width,
            next.text.scroll_interval_ms,
            next.text.title_gap,
        );

        osc.reset_sync(next.text.width * 2)?;

        synchronization_reset = true;
    }

    *current = next;

    Ok(ConfigChange {
        write_timer_changed,
        discovery_changed,
        synchronization_reset,
    })
}

fn request_discovery(
    config: &AppConfig,
    sender: &UnboundedSender<DiscoveryMessage>,
    generation: &mut u64,
    active: &mut Option<u64>,
) {
    *generation = generation.wrapping_add(1);

    let current_generation = *generation;

    *active = None;

    if !config.osc.auto_discover || !config.oscquery.enabled {
        return;
    }

    let oscquery_config = config.oscquery.clone();

    let sender = sender.clone();

    *active = Some(current_generation);

    tokio::spawn(async move {
        let result = match OscQueryClient::new(oscquery_config) {
            Ok(client) => client.discover(DISCOVERY_TIMEOUT).await,

            Err(error) => Err(error),
        };

        let _ = sender.send(DiscoveryMessage {
            generation: current_generation,

            result,
        });
    });
}

fn apply_endpoint(
    endpoint: OscEndpoint,
    osc: &mut OscSender,
    active_target: &mut (String, u16),
) -> bool {
    if active_target.0 == endpoint.host && active_target.1 == endpoint.port {
        return false;
    }

    match osc.set_target(&endpoint.host, endpoint.port) {
        Ok(changed) => {
            *active_target = (endpoint.host, endpoint.port);

            if changed {
                tracing::info!(
                    host =
                        %active_target.0,

                    port =
                        active_target.1,

                    "VRChat OSC target discovered"
                );
            }

            changed
        }

        Err(error) => {
            tracing::warn!(
                host = %endpoint.host,
                port = endpoint.port,
                error = %error,
                "Could not apply discovered OSC target"
            );

            false
        }
    }
}

fn create_composer(config: &AppConfig) -> TextComposer {
    TextComposer::new(
        config.text.width,
        config.text.scroll_interval_ms,
        config.text.title_gap,
    )
}

fn make_write_timer(milliseconds: u64) -> Interval {
    make_timer(
        Duration::from_millis(milliseconds.max(MIN_WRITE_STEP_MS)),
        MissedTickBehavior::Delay,
    )
}

fn make_timer(period: Duration, behavior: MissedTickBehavior) -> Interval {
    let mut timer = interval_at(TokioInstant::now() + period, period);

    timer.set_missed_tick_behavior(behavior);

    timer
}
