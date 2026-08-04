use crate::{
    command::RuntimeCommand, config::ConfigManager, error::AppResult, media::MediaWatcher,
    model::AppConfig, osc::OscSender, oscquery::OscQueryClient, text::TextComposer,
};

use std::time::{Duration, Instant};

use tokio::{
    sync::mpsc::UnboundedReceiver,
    time::{Instant as TokioInstant, Interval, MissedTickBehavior, interval, interval_at},
};

const CONFIG_CHECK_INTERVAL: Duration = Duration::from_secs(1);

const OSC_DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);

const OSC_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

struct ConfigChange {
    update_interval_changed: bool,
    rediscover: bool,
}

pub async fn run(mut commands: UnboundedReceiver<RuntimeCommand>) -> AppResult<()> {
    let mut config = ConfigManager::load()?;

    let mut config_modified = ConfigManager::modified_time()?;

    let media = MediaWatcher::start();

    let mut composer = TextComposer::new(config.text.width, config.text.scroll_interval_ms);

    let mut osc = OscSender::new(&config.osc, &config.parameters, config.text.width * 2)?;

    let mut active_target = (config.osc.host.clone(), config.osc.port);

    let mut update_timer = make_update_timer(config.text.update_interval_ms);

    let mut config_timer = make_delayed_timer(CONFIG_CHECK_INTERVAL);

    let mut discovery_timer = make_delayed_timer(OSC_DISCOVERY_INTERVAL);

    let mut force_refresh = true;
    let mut last_full_refresh = Instant::now();

    if config.osc.auto_discover && discover_and_apply(&config, &mut osc, &mut active_target).await {
        force_refresh = true;
    }

    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(RuntimeCommand::ReloadConfig) => {
                        match ConfigManager::load() {
                            Ok(new_config) => {
                                config_modified =
                                    ConfigManager::modified_time()
                                        .unwrap_or(config_modified);

                                if let Err(error) =
                                    install_config(
                                        new_config,
                                        &mut config,
                                        &mut composer,
                                        &mut osc,
                                        &mut active_target,
                                        &mut update_timer,
                                        &mut force_refresh,
                                    )
                                    .await
                                {
                                    tracing::warn!(
                                        error = %error,
                                        "could not reload configuration"
                                    );
                                }
                            }

                            Err(error) => {
                                tracing::warn!(
                                    error = %error,
                                    "could not read configuration"
                                );
                            }
                        }
                    }

                    Some(RuntimeCommand::Shutdown) |
                    None => {
                        break;
                    }
                }
            }

            _ = config_timer.tick() => {
                match ConfigManager::load_if_changed(
                    &mut config_modified,
                ) {
                    Ok(Some(new_config)) => {
                        if let Err(error) =
                            install_config(
                                new_config,
                                &mut config,
                                &mut composer,
                                &mut osc,
                                &mut active_target,
                                &mut update_timer,
                                &mut force_refresh,
                            )
                            .await
                        {
                            tracing::warn!(
                                error = %error,
                                "could not apply changed configuration"
                            );
                        }
                    }

                    Ok(None) => {}

                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "could not check configuration"
                        );
                    }
                }
            }

            _ = discovery_timer.tick(),
            if config.osc.auto_discover => {
                if discover_and_apply(
                    &config,
                    &mut osc,
                    &mut active_target,
                )
                .await
                {
                    force_refresh = true;
                }
            }

            _ = update_timer.tick() => {
                let media_state =
                    media.borrow().clone();

                let frame =
                    composer.compose(
                        &media_state,
                        &config.text.separator,
                    );

                let characters =
                    composer.frame_to_ids(&frame);

                let full_refresh_due =
                    last_full_refresh.elapsed()
                        >= Duration::from_secs(
                            config
                                .text
                                .full_refresh_seconds
                                .max(5),
                        );

                let result =
                    if force_refresh || full_refresh_due {
                        osc.force_refresh(
                            &characters,
                        )
                        .await
                    } else {
                        osc.send_changed(
                            &characters,
                        )
                        .await
                    };

                match result {
                    Ok(sent) => {
                        if force_refresh || full_refresh_due {
                            force_refresh = false;
                            last_full_refresh =
                                Instant::now();
                        }

                        if sent > 0 {
                            tracing::trace!(
                                sent,
                                "OSC characters sent"
                            );
                        }
                    }

                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "could not send OSC text"
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

async fn install_config(
    new_config: AppConfig,
    config: &mut AppConfig,
    composer: &mut TextComposer,
    osc: &mut OscSender,
    active_target: &mut (String, u16),
    update_timer: &mut Interval,
    force_refresh: &mut bool,
) -> AppResult<()> {
    let change = apply_config(config, &new_config, composer, osc, active_target)?;

    *config = new_config;
    *force_refresh = true;

    if change.update_interval_changed {
        *update_timer = make_update_timer(config.text.update_interval_ms);
    }

    if change.rediscover && discover_and_apply(config, osc, active_target).await {
        *force_refresh = true;
    }

    Ok(())
}

fn apply_config(
    previous: &AppConfig,
    next: &AppConfig,
    composer: &mut TextComposer,
    osc: &mut OscSender,
    active_target: &mut (String, u16),
) -> AppResult<ConfigChange> {
    let update_interval_changed = previous.text.update_interval_ms != next.text.update_interval_ms;

    let text_composer_changed = previous.text.width != next.text.width
        || previous.text.scroll_interval_ms != next.text.scroll_interval_ms;

    let osc_target_changed = previous.osc.host != next.osc.host
        || previous.osc.port != next.osc.port
        || previous.osc.auto_discover != next.osc.auto_discover;

    let parameters_changed = previous.parameters != next.parameters;

    let rediscover =
        next.osc.auto_discover && (previous.osc != next.osc || previous.oscquery != next.oscquery);

    if osc_target_changed {
        osc.set_target(&next.osc.host, next.osc.port)?;

        *active_target = (next.osc.host.clone(), next.osc.port);
    }

    if parameters_changed {
        osc.set_parameters(&next.parameters);
    }

    if text_composer_changed {
        composer.reconfigure(next.text.width, next.text.scroll_interval_ms);
    }

    if previous != next {
        osc.invalidate();
    }

    Ok(ConfigChange {
        update_interval_changed,
        rediscover,
    })
}

async fn discover_and_apply(
    config: &AppConfig,
    osc: &mut OscSender,
    active_target: &mut (String, u16),
) -> bool {
    if !config.osc.auto_discover || !config.oscquery.enabled {
        return false;
    }

    let client = match OscQueryClient::new(config.oscquery.clone()) {
        Ok(client) => client,

        Err(error) => {
            tracing::debug!(
                error = %error,
                "could not initialize OSCQuery client"
            );

            return false;
        }
    };

    let endpoint = match client.discover(OSC_DISCOVERY_TIMEOUT).await {
        Ok(Some(endpoint)) => endpoint,

        Ok(None) => {
            tracing::debug!("VRChat OSCQuery service was not found");

            return false;
        }

        Err(error) => {
            tracing::debug!(
                error = %error,
                "OSCQuery discovery failed"
            );

            return false;
        }
    };

    if active_target.0.as_str() == endpoint.host.as_str() && active_target.1 == endpoint.port {
        return false;
    }

    if let Err(error) = osc.set_target(&endpoint.host, endpoint.port) {
        tracing::warn!(
            host = %endpoint.host,
            port = endpoint.port,
            error = %error,
            "could not use discovered OSC target"
        );

        return false;
    }

    tracing::info!(
        host = %endpoint.host,
        port = endpoint.port,
        "VRChat OSC target discovered"
    );

    *active_target = (endpoint.host, endpoint.port);

    true
}

fn make_update_timer(milliseconds: u64) -> Interval {
    let mut timer = interval(Duration::from_millis(milliseconds.max(50)));

    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    timer
}

fn make_delayed_timer(period: Duration) -> Interval {
    let mut timer = interval_at(TokioInstant::now() + period, period);

    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    timer
}
