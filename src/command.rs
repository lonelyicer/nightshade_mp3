#[derive(Clone, Copy, Debug)]
pub enum RuntimeCommand {
    ReloadConfig,
    Shutdown,
}
