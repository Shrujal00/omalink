// omalink — your Android phone on your Omarchy desktop: messages, calls,
// notifications, and the clipboard, over Bluetooth.

mod bluez;
mod bridge;
mod core;
mod demo;
mod desktop;
mod link;
mod telephony;
mod transfer;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

extern "C" {
    fn omalink_share_gl_contexts();
}

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub demo: bool,
    pub background: bool,
}

static OPTIONS: std::sync::OnceLock<Options> = std::sync::OnceLock::new();

pub fn options() -> Options {
    OPTIONS.get().copied().unwrap_or_default()
}

fn main() {
    let mut options = Options::default();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--demo" => options.demo = true,
            "--background" => options.background = true,
            "--version" | "-V" => {
                println!("omalink {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" => {
                println!(
                    "omalink {}\nYour Android phone on your desktop.\n\n  omalink               open the window (or raise it if already running)\n  omalink --background  start without a window, for autostart\n  omalink --demo        show the interface with made-up data\n  omalink --version",
                    env!("CARGO_PKG_VERSION")
                );
                return;
            }
            other => {
                eprintln!("omalink: unknown option {other}");
                std::process::exit(1);
            }
        }
    }
    let _ = OPTIONS.set(options);

    // Must happen before the application exists; see glshare.cpp.
    unsafe { omalink_share_gl_contexts() };

    let mut app = QGuiApplication::new();
    if let Some(mut app) = app.as_mut() {
        // The binary's name is also the Wayland app id, which is how the
        // compositor matches the window to omalink.desktop and its icon.
        app.as_mut().set_application_name(&QString::from("omalink"));
        app.as_mut().set_application_display_name(&QString::from("Omalink"));
        app.as_mut().set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));
    }
    // Every control is drawn by the app, so the lightest style will do.
    QQuickStyle::set_style(&QString::from("Basic"));

    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/omalink/qml/Main.qml"));
    }
    let code = app.as_mut().map(|app| app.exec()).unwrap_or(1);
    // Leave without running destructors: the core thread is still parked on
    // its sockets and Qt's teardown would wait on it forever. Everything
    // worth keeping (the config) is written the moment it changes.
    unsafe { libc::_exit(code) }
}
