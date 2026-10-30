//! ArcShot's native menu-bar application.

mod coordinator;
mod logging;
mod ui;

use std::{cell::RefCell, rc::Rc, time::Duration};

use anyhow::{Result, anyhow, ensure};
use arcshot_core::MenuBarText;
use arcshot_macos::MenuBar;
use gpui::{App, Application, Global, Timer, px};

/// A readable probe size with distinct Helvetica advances; changing it changes reported metrics.
const TEXT_PROBE_FONT_SIZE: f32 = 16.0;
/// Allows native startup to settle before revalidation; this is not a performance threshold.
const STARTUP_SMOKE_DELAY: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunMode {
    Resident,
    StartupSmoke,
}

struct MenuBarGlobal(MenuBar);

impl Global for MenuBarGlobal {}

fn check_text_backend() -> Result<()> {
    let application = Application::new();
    let text_system = application.text_system();
    let font_id = text_system.resolve_font(&gpui::font("Helvetica"));
    let narrow = text_system
        .advance(font_id, px(TEXT_PROBE_FONT_SIZE), 'i')?
        .width;
    let wide = text_system
        .advance(font_id, px(TEXT_PROBE_FONT_SIZE), 'W')?
        .width;

    ensure!(
        wide > narrow,
        "inactive or synthetic text backend: Helvetica W advance {wide:?} must exceed i advance {narrow:?}"
    );
    println!("text_backend=pass font=Helvetica i_advance={narrow:?} W_advance={wide:?}");
    Ok(())
}

fn run_app(mode: RunMode) -> Result<()> {
    let error_slot = Rc::new(RefCell::new(None::<String>));
    let launch_error_slot = Rc::clone(&error_slot);

    Application::new().run(move |cx: &mut App| {
        let menu_bar = match MenuBar::try_new(arcshot_image::status_item_icon(), MenuBarText::new())
        {
            Ok(menu_bar) => menu_bar,
            Err(error) => {
                *launch_error_slot.borrow_mut() = Some(error.to_string());
                cx.quit();
                return;
            }
        };
        cx.set_global(MenuBarGlobal(menu_bar));

        if mode == RunMode::Resident {
            coordinator::launch(cx);
        }

        if mode == RunMode::StartupSmoke {
            let smoke_error_slot = Rc::clone(&launch_error_slot);
            cx.spawn(async move |cx| {
                Timer::after(STARTUP_SMOKE_DELAY).await;
                let result = cx.update(|cx| -> Result<()> {
                    let menu_bar = &cx.global::<MenuBarGlobal>().0;
                    menu_bar.validate_capture_commands()?;
                    ensure!(
                        arcshot_macos::owned_visible_window_ids()?.is_empty(),
                        "status-item-only smoke must not expose a capture window"
                    );
                    println!("capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true");
                    println!("capture_menu_smoke=pass queued_commands=2 no_native_capture=true");
                    println!(
                        "startup_smoke=pass activation_policy=Accessory quit_action=terminate:"
                    );
                    Ok(menu_bar.perform_quit_action()?)
                });
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        *smoke_error_slot.borrow_mut() = Some(error.to_string());
                        let _ = cx.update(|cx| cx.quit());
                    }
                    Err(error) => {
                        *smoke_error_slot.borrow_mut() =
                            Some(format!("could not dispatch startup smoke action: {error}"));
                    }
                }
            })
            .detach();
        }
    });

    match error_slot.borrow_mut().take() {
        Some(error) => Err(anyhow!(error)),
        None => Ok(()),
    }
}

fn main() -> Result<()> {
    logging::init().map_err(|error| anyhow!("could not initialize diagnostics: {error}"))?;
    let mut mode = RunMode::Resident;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--check-text-backend" => return check_text_backend(),
            "--check-capture-environment" => {
                let display = arcshot_macos::builtin_display()?;
                println!(
                    "capture_environment display_id={} origin={},{} logical={}x{} scale={} screen_recording={} guide_seen={}",
                    display.id,
                    display.x,
                    display.y,
                    display.width,
                    display.height,
                    display.scale,
                    arcshot_macos::screen_recording_available(),
                    arcshot_macos::permission_guide_seen()
                );
                return Ok(());
            }
            "--smoke-test" => mode = RunMode::StartupSmoke,
            _ => return Err(anyhow!("unrecognized argument: {argument}")),
        }
    }

    run_app(mode)
}
