use crate::{
    error::{AppError, AppResult},
    model::{MediaInfo, MediaState},
};

use gsmtc::{
    ManagerEvent,
    PlaybackStatus,
    SessionManager,
    SessionModel,
    SessionUpdateEvent,
};

use std::{
    collections::HashMap,
    time::{
        Duration,
        SystemTime,
        UNIX_EPOCH,
    },
};

use tokio::sync::{
    mpsc,
    watch,
};

const WINDOWS_TICKS_PER_SECOND: f64 =
    10_000_000.0;

const RESTART_DELAY: Duration =
    Duration::from_secs(2);

pub struct MediaWatcher;

struct SessionMessage {
    session_id: usize,
    model: SessionModel,
}

impl MediaWatcher {
    pub fn start() -> watch::Receiver<MediaState> {
        let (sender, receiver) =
            watch::channel(MediaState::default());

        tokio::spawn(async move {
            loop {
                if let Err(error) =
                    run_media_manager(
                        sender.clone(),
                    )
                        .await
                {
                    tracing::warn!(
                        error = %error,
                        "Media manager stopped"
                    );
                }

                let _ =
                    sender.send(
                        MediaState::default(),
                    );

                tokio::time::sleep(
                    RESTART_DELAY,
                )
                    .await;
            }
        });

        receiver
    }
}

async fn run_media_manager(
    sender: watch::Sender<MediaState>,
) -> AppResult<()> {
    let mut manager_events =
        SessionManager::create()
            .await
            .map_err(|error| {
                AppError::Message(
                    format!(
                        "Could not create the GSMTC session manager: {error}"
                    ),
                )
            })?;

    let (
        session_sender,
        mut session_receiver,
    ) = mpsc::unbounded_channel::<SessionMessage>();

    let mut sessions =
        HashMap::<usize, SessionModel>::new();

    let mut current_session =
        None::<usize>;

    loop {
        tokio::select! {
            manager_event =
                manager_events.recv() =>
            {
                let Some(manager_event) =
                    manager_event
                else {
                    return Err(
                        AppError::Message(
                            "The GSMTC manager event stream ended."
                                .to_owned(),
                        ),
                    );
                };

                match manager_event {
                    ManagerEvent::SessionCreated {
                        session_id,
                        mut rx,
                        source,
                    } => {
                        tracing::debug!(
                            session_id,
                            source = %source,
                            "Media session created"
                        );

                        let session_sender =
                            session_sender.clone();

                        tokio::spawn(async move {
                            while let Some(event) =
                                rx.recv().await
                            {
                                let model =
                                    match event {
                                        SessionUpdateEvent::Model(
                                            model,
                                        ) => model,

                                        SessionUpdateEvent::Media(
                                            model,
                                            _image,
                                        ) => model,
                                    };

                                if session_sender
                                    .send(
                                        SessionMessage {
                                            session_id,
                                            model,
                                        },
                                    )
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        });
                    }

                    ManagerEvent::SessionRemoved {
                        session_id,
                    } => {
                        sessions.remove(
                            &session_id,
                        );

                        if current_session
                            == Some(session_id)
                        {
                            current_session =
                                None;
                        }

                        publish_state(
                            &sender,
                            &sessions,
                            current_session,
                        );
                    }

                    ManagerEvent::CurrentSessionChanged {
                        session_id,
                    } => {
                        current_session =
                            session_id;

                        publish_state(
                            &sender,
                            &sessions,
                            current_session,
                        );
                    }
                }
            }

            session_message =
                session_receiver.recv() =>
            {
                let Some(session_message) =
                    session_message
                else {
                    return Err(
                        AppError::Message(
                            "The GSMTC session update stream ended."
                                .to_owned(),
                        ),
                    );
                };

                sessions.insert(
                    session_message.session_id,
                    session_message.model,
                );

                publish_state(
                    &sender,
                    &sessions,
                    current_session,
                );
            }
        }
    }
}

fn publish_state(
    sender: &watch::Sender<MediaState>,
    sessions: &HashMap<usize, SessionModel>,
    current_session: Option<usize>,
) {
    let info =
        select_session(
            sessions,
            current_session,
        )
            .map(session_to_media_info)
            .unwrap_or_default();

    tracing::debug!(
        title = %info.title,
        artist = %info.artist,
        position = info.position,
        duration = info.duration,
        playing = info.playing,
        "Media state updated"
    );

    let mut state =
        MediaState::default();

    state.update(info);

    let _ =
        sender.send(state);
}

fn select_session<'a>(
    sessions: &'a HashMap<usize, SessionModel>,
    current_session: Option<usize>,
) -> Option<&'a SessionModel> {
    if let Some(session_id) =
        current_session
    {
        if let Some(session) =
            sessions.get(&session_id)
        {
            if has_media(session) {
                return Some(session);
            }
        }
    }

    sessions
        .values()
        .find(|session| {
            has_media(session)
                && is_playing(session)
        })
        .or_else(|| {
            sessions
                .values()
                .find(|session| {
                    has_media(session)
                })
        })
}

fn has_media(
    session: &SessionModel,
) -> bool {
    session
        .media
        .as_ref()
        .is_some_and(|media| {
            !media.title.trim().is_empty()
                || !media.artist.trim().is_empty()
        })
}

fn is_playing(
    session: &SessionModel,
) -> bool {
    session
        .playback
        .as_ref()
        .is_some_and(|playback| {
            playback.status
                == PlaybackStatus::Playing
        })
}

fn session_to_media_info(
    session: &SessionModel,
) -> MediaInfo {
    let Some(media) =
        session.media.as_ref()
    else {
        return MediaInfo::default();
    };

    let playing =
        is_playing(session);

    let (
        position,
        duration,
    ) =
        timeline_to_seconds(
            session,
            playing,
        );

    MediaInfo {
        title:
        media.title.trim().to_owned(),

        artist:
        media.artist.trim().to_owned(),

        position,

        duration,

        playing,
    }
}

fn timeline_to_seconds(
    session: &SessionModel,
    playing: bool,
) -> (f64, f64) {
    let Some(timeline) =
        session.timeline.as_ref()
    else {
        return (
            0.0,
            0.0,
        );
    };

    let duration_ticks =
        timeline
            .end
            .saturating_sub(
                timeline.start,
            )
            .max(0);

    let position_ticks =
        timeline
            .position
            .saturating_sub(
                timeline.start,
            )
            .max(0);

    let duration =
        duration_ticks as f64
            / WINDOWS_TICKS_PER_SECOND;

    let mut position =
        position_ticks as f64
            / WINDOWS_TICKS_PER_SECOND;

    if playing
        && timeline.last_updated_at_ms > 0
    {
        let elapsed_milliseconds =
            unix_time_milliseconds()
                .saturating_sub(
                    timeline.last_updated_at_ms,
                )
                .max(0);

        let playback_rate =
            session
                .playback
                .as_ref()
                .map(|playback| {
                    playback.rate
                })
                .filter(|rate| {
                    rate.is_finite()
                        && *rate > 0.0
                })
                .unwrap_or(1.0);

        position +=
            elapsed_milliseconds as f64
                / 1000.0
                * playback_rate;
    }

    if duration > 0.0 {
        position =
            position.clamp(
                0.0,
                duration,
            );
    } else {
        position =
            position.max(0.0);
    }

    (
        position,
        duration,
    )
}

fn unix_time_milliseconds() -> i64 {
    SystemTime::now()
        .duration_since(
            UNIX_EPOCH,
        )
        .ok()
        .and_then(|duration| {
            i64::try_from(
                duration.as_millis(),
            )
                .ok()
        })
        .unwrap_or_default()
}