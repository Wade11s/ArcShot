use std::{cell::RefCell, rc::Rc, time::Duration};

use anyhow::{Result, anyhow, ensure};
use arcshot_core::MenuBarText;
use arcshot_macos::MenuBar;
use gpui::{App, Application, Global, Timer, px};

struct MenuBarGlobal(MenuBar);

impl Global for MenuBarGlobal {}

fn check_text_backend() -> Result<()> {
    let application = Application::new();
    let text_system = application.text_system();
    let font_id = text_system.resolve_font(&gpui::font("Helvetica"));
    let narrow = text_system.advance(font_id, px(16.0), 'i')?.width;
    let wide = text_system.advance(font_id, px(16.0), 'W')?.width;

    ensure!(
        wide > narrow,
        "inactive or synthetic text backend: Helvetica W advance {wide:?} must exceed i advance {narrow:?}"
    );
    println!("text_backend=pass font=Helvetica i_advance={narrow:?} W_advance={wide:?}");
    Ok(())
}

fn run_app(smoke_test: bool) -> Result<()> {
    let error_slot = Rc::new(RefCell::new(None::<String>));
    let launch_error_slot = Rc::clone(&error_slot);

    Application::new().run(move |cx: &mut App| {
        let menu_bar = match MenuBar::new(arcshot_image::status_item_icon(), MenuBarText::default())
        {
            Ok(menu_bar) => menu_bar,
            Err(error) => {
                *launch_error_slot.borrow_mut() = Some(error.to_string());
                cx.quit();
                return;
            }
        };
        cx.set_global(MenuBarGlobal(menu_bar));

        if smoke_test {
            let smoke_error_slot = Rc::clone(&launch_error_slot);
            cx.spawn(async move |cx| {
                Timer::after(Duration::from_millis(200)).await;
                let result = cx.update(|cx| {
                    let menu_bar = &cx.global::<MenuBarGlobal>().0;
                    menu_bar.validate()?;
                    println!(
                        "startup_smoke=pass activation_policy=Accessory quit_action=terminate:"
                    );
                    menu_bar.perform_quit_action()
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
    let mut smoke_test = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--check-text-backend" => return check_text_backend(),
            "--smoke-test" => smoke_test = true,
            _ => return Err(anyhow!("unrecognized argument: {argument}")),
        }
    }

    run_app(smoke_test)
}
