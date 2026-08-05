use crate::{
    command::RuntimeCommand,
    config::ConfigManager,
    error::AppResult,
    media::MediaWatcher,
    model::AppConfig,
    osc::{OscSender, WriteEvent},
    oscquery::{OscEndpoint, OscQueryClient},
    text::TextComposer,
};

use std::time::{Duration, Instant};

use tokio::{
    sync::mpsc::UnboundedReceiver,
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

    let mut discovery_timer = make_timer(DISCOVERY_INTERVAL, MissedTickBehavior::Delay);

    if config.osc.auto_discover && config.oscquery.enabled {
        discover_and_apply(&config, &mut osc, &mut active_target).await;
    }

    tracing::info!(
        write_step_ms = config.text.write_step_ms,
        scroll_interval_ms = config.text.scroll_interval_ms,
        slots = composer.buffer_length(),
        "Backend runtime started"
    );

    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(
                        RuntimeCommand::ReloadConfig,
                    ) => {
                        reload_config(
                            &mut config,
                            &mut config_modified,
                            &mut composer,
                            &mut osc,
                            &mut active_target,
                            &mut write_timer,
                        )
                        .await;
                    }

                    Some(
                        RuntimeCommand::Shutdown,
                    )
                    | None => {
                        break;
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
                                if change.write_timer_changed {
                                    write_timer =
                                        make_write_timer(
                                            config
                                                .text
                                                .write_step_ms,
                                        );
                                }

                                if change.discovery_changed
                                    && config
                                        .osc
                                        .auto_discover
                                    && config
                                        .oscquery
                                        .enabled
                                {
                                    discover_and_apply(
                                        &config,
                                        &mut osc,
                                        &mut active_target,
                                    )
                                    .await;
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

            _ = discovery_timer.tick(),
            if config.osc.auto_discover
                && config.oscquery.enabled
            => {
                discover_and_apply(
                    &config,
                    &mut osc,
                    &mut active_target,
                )
                .await;
            }

            _ = write_timer.tick() => {
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

                    match osc.begin_frame(
                        &frame.characters,
                    ) {
                        Ok(true) => {
                            tracing::trace!(
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
                                "Could not start display frame"
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
                            "OSC character latched"
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
                        },
                    ) => {
                        tracing::trace!(
                            slot,
                            character,
                            "Frozen display frame completed"
                        );
                    }

                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "Could not advance OSC writer"
                        );
                    }
                }
            }
        }
    }

    tracing::info!("Backend runtime stopped");

    Ok(())
}

async fn reload_config(
    config: &mut AppConfig,
    config_modified: &mut Option<std::time::SystemTime>,
    composer: &mut TextComposer,
    osc: &mut OscSender,
    active_target: &mut (String, u16),
    write_timer: &mut Interval,
) {
    let next = match ConfigManager::load() {
        Ok(config) => config,

        Err(error) => {
            tracing::warn!(
                error = %error,
                "Could not reload configuration"
            );

            return;
        }
    };

    match ConfigManager::modified_time() {
        Ok(modified) => {
            *config_modified = modified;
        }

        Err(error) => {
            tracing::debug!(
                error = %error,
                "Could not read configuration modification time"
            );
        }
    }

    match apply_config(next, config, composer, osc, active_target) {
        Ok(change) => {
            if change.write_timer_changed {
                *write_timer = make_write_timer(config.text.write_step_ms);
            }

            if change.discovery_changed && config.osc.auto_discover && config.oscquery.enabled {
                discover_and_apply(config, osc, active_target).await;
            }

            tracing::info!("Configuration reloaded");
        }

        Err(error) => {
            tracing::warn!(
                error = %error,
                "Could not apply reloaded configuration"
            );
        }
    }
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

    if configured_target_changed || automatic_discovery_disabled {
        osc.set_target(&next.osc.host, next.osc.port)?;

        *active_target = (next.osc.host.clone(), next.osc.port);

        tracing::info!(
            host = %next.osc.host,
            port = next.osc.port,
            "Configured OSC target applied"
        );
    }

    if parameters_changed {
        osc.set_parameters(&next.parameters)?;
    }

    if text_changed {
        composer.reconfigure(
            next.text.width,
            next.text.scroll_interval_ms,
            next.text.title_gap,
        );

        osc.reset_sync(next.text.width * 2)?;
    }

    *current = next;

    Ok(ConfigChange {
        write_timer_changed,
        discovery_changed,
    })
}

async fn discover_and_apply(
    config: &AppConfig,
    osc: &mut OscSender,
    active_target: &mut (String, u16),
) {
    let client = match OscQueryClient::new(config.oscquery.clone()) {
        Ok(client) => client,

        Err(error) => {
            tracing::debug!(
                error = %error,
                "Could not initialize OSCQuery"
            );

            return;
        }
    };

    let endpoint = match client.discover(DISCOVERY_TIMEOUT).await {
        Ok(Some(endpoint)) => endpoint,

        Ok(None) => {
            tracing::debug!("VRChat was not discovered through OSCQuery");

            return;
        }

        Err(error) => {
            tracing::debug!(
                error = %error,
                "OSCQuery discovery failed"
            );

            return;
        }
    };

    apply_endpoint(endpoint, osc, active_target);
}

fn apply_endpoint(endpoint: OscEndpoint, osc: &mut OscSender, active_target: &mut (String, u16)) {
    if active_target.0 == endpoint.host && active_target.1 == endpoint.port {
        return;
    }

    match osc.set_target(&endpoint.host, endpoint.port) {
        Ok(()) => {
            tracing::info!(
                host = %endpoint.host,
                port = endpoint.port,
                "VRChat OSC target discovered"
            );

            *active_target = (endpoint.host, endpoint.port);
        }

        Err(error) => {
            tracing::warn!(
                host = %endpoint.host,
                port = endpoint.port,
                error = %error,
                "Could not apply discovered OSC target"
            );
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
