//! ArcShot's native menu-bar application.

mod coordinator;
mod logging;
mod ui;

use std::{cell::RefCell, rc::Rc, time::Duration};

use anyhow::{Result, anyhow, ensure};
use arcshot_core::MenuBarText;
use arcshot_macos::MenuBar;
use gpui::{App, Global, px};

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
    let application = gpui_platform::headless();
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

    gpui_platform::application().run(move |cx: &mut App| {
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
                cx.background_executor().timer(STARTUP_SMOKE_DELAY).await;
                let result = cx.update(|cx| -> Result<()> {
                    let menu_bar = &cx.global::<MenuBarGlobal>().0;
                    menu_bar.validate_capture_commands()?;
                    ensure!(
                        arcshot_macos::owned_visible_window_ids()?.is_empty(),
                        "status-item-only smoke must not expose a capture window"
                    );
                    println!("capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true");
                    println!("capture_menu_smoke=pass queued_commands=5 no_native_capture=true");
                    // The two preference items show the stored sound value and the
                    // real SMAppService login-item status; the smoke only reads.
                    let login_item = menu_bar.validate_preference_items()?;
                    println!(
                        "preference_items_smoke=pass sound_selector=toggleCompletionSound: login_selector=toggleLaunchAtLogin: state_from_system=true"
                    );
                    println!(
                        "login_item_status_smoke=pass state={login_item:?} read_only=true"
                    );
                    // The stored completion-sound value round-trips in the real
                    // per-user preferences domain and is restored afterwards.
                    let original = arcshot_macos::completion_sound_enabled();
                    arcshot_macos::store_completion_sound(!original);
                    ensure!(
                        arcshot_macos::completion_sound_enabled() == !original,
                        "completion-sound preference did not round-trip"
                    );
                    arcshot_macos::store_completion_sound(original);
                    ensure!(
                        arcshot_macos::completion_sound_enabled() == original,
                        "completion-sound preference was not restored"
                    );
                    println!(
                        "preference_smoke=pass key=CompletionSoundEnabled roundtrip=true restored=true"
                    );
                    // Native hotkey registration, suspension and cleanup; no
                    // key event is synthesized, so no session can start.
                    let mut hotkeys = arcshot_macos::GlobalHotkeys::install()?;
                    let default = arcshot_core::Hotkey::default();
                    hotkeys.set_active(default)?;
                    hotkeys.suspend()?;
                    hotkeys.resume()?;
                    drop(hotkeys);
                    println!("hotkey_registration_smoke=pass default={default} suspend_resume_cleanup=clean");
                    // Native PNGf path plus AppKit in-memory decode on a private
                    // pasteboard; the user's clipboard is never modified.
                    let composed = arcshot_image::ComposedImage::try_new(2, 2, vec![255; 16])?;
                    let png = arcshot_image::encode_png(&composed)?;
                    let decoded = arcshot_macos::validate_png_pasteboard(&png, 2, 2)?;
                    println!(
                        "clipboard_pngf_smoke=pass width={} height={} private_pasteboard=true",
                        decoded.width, decoded.height
                    );
                    // Explicit save writes the same encoded bytes to a real file,
                    // verifies the roundtrip and removes the probe file. Rejected
                    // destinations and non-PNG bytes never claim success.
                    let save_path = std::env::temp_dir()
                        .join(format!("arcshot-save-smoke-{}.png", std::process::id()));
                    arcshot_macos::write_png_to_file(&save_path, &png)?;
                    ensure!(
                        std::fs::read(&save_path)? == png,
                        "saved PNG did not read back byte-identically"
                    );
                    std::fs::remove_file(&save_path)?;
                    let missing = std::env::temp_dir().join(format!(
                        "arcshot-save-smoke-missing-{}/out.png",
                        std::process::id()
                    ));
                    ensure!(
                        arcshot_macos::write_png_to_file(&missing, &png).is_err()
                            && !missing.exists(),
                        "a missing destination must fail without creating a file"
                    );
                    ensure!(
                        arcshot_macos::write_png_to_file(&save_path, b"not a png").is_err()
                            && !save_path.exists(),
                        "non-PNG bytes must not create a file"
                    );
                    // Configures the real PNG panel without showing it; showing and
                    // cancelling it is a MAC-RECOVERY manual case.
                    drop(arcshot_macos::PngSavePanel::try_new("ArcShot.png")?);
                    println!(
                        "save_file_smoke=pass bytes={} roundtrip=true rejected_destination=true invalid_bytes=true panel_configured=true",
                        png.len()
                    );
                    println!(
                        "startup_smoke=pass activation_policy=Accessory quit_action=terminate:"
                    );
                    Ok(menu_bar.perform_quit_action()?)
                });
                match result {
                    Ok(()) => {}
                    Err(error) => {
                        *smoke_error_slot.borrow_mut() = Some(error.to_string());
                        cx.update(|cx| cx.quit());
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
