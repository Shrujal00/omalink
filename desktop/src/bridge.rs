//! The one object the window sees. State flows out as `update(topic, json)`;
//! intent flows in as `send(command, json)`. Everything behind it runs on its
//! own thread, so the interface never waits on Bluetooth.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type Backend = super::BackendRust;

        /// Starts the core. Called once, after the window has connected to
        /// `update`, so nothing emitted at startup is missed.
        #[qinvokable]
        fn start(self: Pin<&mut Backend>);

        #[qinvokable]
        fn send(self: Pin<&mut Backend>, command: &QString, args: &QString);

        #[qsignal]
        fn update(self: Pin<&mut Backend>, topic: QString, payload: QString);
    }

    impl cxx_qt::Threading for Backend {}
}

use core::pin::Pin;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use tokio::sync::mpsc::UnboundedSender;

use crate::core::{Emit, Event};

#[derive(Default)]
pub struct BackendRust {
    tx: Option<UnboundedSender<Event>>,
}

impl qobject::Backend {
    fn start(mut self: Pin<&mut Self>) {
        if self.tx.is_some() {
            return;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.as_mut().rust_mut().tx = Some(tx.clone());

        let qt = self.qt_thread();
        let emit: Emit = Arc::new(move |topic: &str, payload: String| {
            let topic = topic.to_string();
            let _ = qt.queue(move |backend| backend.update(QString::from(&topic), QString::from(&payload)));
        });

        let options = crate::options();
        std::thread::Builder::new()
            .name("core".into())
            .spawn(move || {
                // One thread is plenty: the work is waiting on D-Bus and a socket.
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
                runtime.block_on(crate::core::run(emit, tx, rx, options.demo, options.background));
            })
            .expect("core thread");
    }

    fn send(self: Pin<&mut Self>, command: &QString, args: &QString) {
        let args = serde_json::from_str(&args.to_string()).unwrap_or(serde_json::Value::Null);
        if let Some(tx) = &self.tx {
            let _ = tx.send(Event::Ui(command.to_string(), args));
        }
    }
}
