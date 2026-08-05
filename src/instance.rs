use crate::error::{AppError, AppResult};

use single_instance::SingleInstance;

const MAIN_INSTANCE_NAME: &str = r"Local\NightshadeMp3.Main.80B7FC99-5532-41DF-86D6-61D741327B43";

pub enum InstanceState {
    Primary(AppInstance),
    AlreadyRunning,
}

pub struct AppInstance {
    _guard: SingleInstance,
}

impl AppInstance {
    pub fn acquire() -> AppResult<InstanceState> {
        let guard = SingleInstance::new(MAIN_INSTANCE_NAME).map_err(|error| {
            AppError::Message(format!(
                "Could not create the application instance lock: {error}"
            ))
        })?;

        if !guard.is_single() {
            return Ok(InstanceState::AlreadyRunning);
        }

        Ok(InstanceState::Primary(Self { _guard: guard }))
    }
}
