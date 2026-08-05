use crate::{
    command::RuntimeCommand,
    error::{AppError, AppResult},
    settings,
};

use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder},
};

use tokio::sync::mpsc::UnboundedSender;

use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
}

pub fn run(runtime_sender: UnboundedSender<RuntimeCommand>) -> AppResult<()> {
    let mut event_loop_builder = EventLoopBuilder::<UserEvent>::with_user_event();

    let event_loop = event_loop_builder.build();

    let event_proxy = event_loop.create_proxy();

    MenuEvent::set_event_handler(Some(move |event| {
        let _ = event_proxy.send_event(UserEvent::Menu(event));
    }));

    let menu = Menu::new();

    let settings_item = MenuItem::new("Settings", true, None);

    let reload_item = MenuItem::new("Reload Configuration", true, None);

    let exit_item = MenuItem::new("Exit", true, None);

    menu.append(&settings_item).map_err(menu_error)?;

    menu.append(&reload_item).map_err(menu_error)?;

    menu.append(&exit_item).map_err(menu_error)?;

    let settings_id = settings_item.id().clone();

    let reload_id = reload_item.id().clone();

    let exit_id = exit_item.id().clone();

    let menu_items = (settings_item, reload_item, exit_item);

    let mut menu_slot = Some(menu);

    let tray_image = crate::icon::tray_icon()?;

    let mut icon_slot = Some(tray_image);

    let mut tray_icon = None::<TrayIcon>;

    event_loop.run(move |event, _event_loop_target, control_flow| {
        *control_flow = ControlFlow::Wait;

        let _keep_menu_items_alive = &menu_items;

        match event {
            Event::NewEvents(StartCause::Init) => {
                if tray_icon.is_some() {
                    return;
                }

                let Some(menu) = menu_slot.take() else {
                    return;
                };

                let Some(icon) = icon_slot.take() else {
                    return;
                };

                match TrayIconBuilder::new()
                    .with_tooltip("Nightshade MP3")
                    .with_icon(icon)
                    .with_menu(Box::new(menu))
                    .build()
                {
                    Ok(icon) => {
                        tray_icon = Some(icon);
                    }

                    Err(error) => {
                        tracing::error!(
                            error = %error,
                            "Could not create the tray icon"
                        );

                        *control_flow = ControlFlow::ExitWithCode(1);
                    }
                }
            }

            Event::UserEvent(UserEvent::Menu(menu_event)) => {
                if menu_event.id() == &settings_id {
                    if let Err(error) = settings::launch() {
                        tracing::error!(
                            error = %error,
                            "Could not open the settings window"
                        );
                    }

                    return;
                }

                if menu_event.id() == &reload_id {
                    let _ = runtime_sender.send(RuntimeCommand::ReloadConfig);

                    return;
                }

                if menu_event.id() == &exit_id {
                    let _ = runtime_sender.send(RuntimeCommand::Shutdown);

                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::LoopDestroyed => {
                let _ = runtime_sender.send(RuntimeCommand::Shutdown);
            }

            _ => {}
        }
    })
}

fn menu_error<E>(error: E) -> AppError
where
    E: std::fmt::Display,
{
    AppError::Message(error.to_string())
}
