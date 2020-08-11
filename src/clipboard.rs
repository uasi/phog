use std::iter;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};

#[cfg(not(any(windows, target_os = "macos")))]
use copypasta::{nop_clipboard::NopClipboardContext as ClipboardContext, ClipboardProvider};
#[cfg(any(windows, target_os = "macos"))]
use copypasta::{ClipboardContext, ClipboardProvider};

pub fn spawn_watcher() -> Receiver<Option<String>> {
    let mut changes_iter = {
        let mut text = String::new();
        iter::from_fn(move || {
            let new_text = read().unwrap_or_else(|e| {
                log::error!("clipboard error: {}", e);
                String::new()
            });
            if new_text != text {
                text = new_text;
                Some(text.clone())
            } else {
                None
            }
        })
    };

    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let handle = unsafe {
        signal_hook::register(signal_hook::SIGINT, move || {
            stop.store(true, Ordering::SeqCst);
        })
    }
    .expect("Failed to set Ctrl-C handler");

    let (tx, rx) = channel();

    thread::spawn(move || loop {
        if let Some(text) = changes_iter.next() {
            tx.send(Some(text)).expect("send must succeed");
        }

        if stopped.load(Ordering::SeqCst) {
            tx.send(None).expect("send must succeed");
            signal_hook::unregister(handle);
            break;
        }

        thread::sleep(Duration::from_secs(1));
    });

    rx
}

pub fn read() -> Result<String> {
    let mut context =
        ClipboardContext::new().map_err(|e| anyhow!("Could not get clipboard context: {}", e))?;
    context
        .get_contents()
        .map_err(|e| anyhow!("Could not read clipboard content: {}", e))
}
