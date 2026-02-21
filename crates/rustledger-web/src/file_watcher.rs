use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{debug, error, info, warn};

use crate::handlers::AppState;

/// File change event
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    #[allow(dead_code)]
    pub path: PathBuf,
    #[allow(dead_code)]
    pub kind: String,
}

/// Spawn a file watcher that monitors the ledger file and its directory
pub fn spawn_file_watcher(state: Arc<AppState>) -> anyhow::Result<()> {
    let ledger_path = state.ledger_path.clone();
    let tx = state.file_change_tx.clone();

    // Get the directory to watch (watch the entire directory for partitioned files)
    let watch_dir = ledger_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| ledger_path.clone());

    // Clone for use in closure
    let watch_dir_clone = watch_dir.clone();
    let ledger_path_clone = ledger_path.clone();

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                error!("Failed to create runtime for file watcher: {}", e);
                return;
            }
        };

        rt.block_on(async {
            let tx_clone = tx.clone();

            let mut watcher = match RecommendedWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    match res {
                        Ok(event) => {
                            let should_notify = event.paths.iter().any(|p| {
                                // Check if this file is relevant to our ledger
                                let is_main_file = p == &ledger_path_clone;
                                let is_beancount_file = p
                                    .extension()
                                    .map(|ext| ext == "beancount")
                                    .unwrap_or(false);
                                let is_in_same_dir = p
                                    .parent()
                                    .map(|parent| parent == watch_dir_clone)
                                    .unwrap_or(false);

                                // Notify for main file, accounts file, or partitioned files
                                is_main_file
                                    || (is_beancount_file
                                        && is_in_same_dir
                                        && is_relevant_event(&event))
                            });

                            if should_notify {
                                debug!("File change detected: {:?}", event);
                                let change_event = FileChangeEvent {
                                    path: event.paths.first().cloned().unwrap_or_default(),
                                    kind: format!("{:?}", event.kind),
                                };
                                let _ = tx_clone.try_send(change_event);
                            }
                        }
                        Err(e) => {
                            warn!("File watcher error: {}", e);
                        }
                    }
                },
                Config::default().with_poll_interval(Duration::from_secs(1)),
            ) {
                Ok(w) => w,
                Err(e) => {
                    error!("Failed to create file watcher: {}", e);
                    return;
                }
            };

            // Watch the directory recursively
            if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
                warn!("Failed to watch directory {:?}: {}", watch_dir, e);
                // Try watching just the file
                if let Err(e) = watcher.watch(&ledger_path, RecursiveMode::NonRecursive) {
                    error!("Failed to watch file {:?}: {}", ledger_path, e);
                    return;
                }
            }

            info!("File watcher started for: {:?}", watch_dir);

            // Keep the watcher alive
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });
    });

    Ok(())
}

/// Check if the event is relevant (not just metadata changes)
fn is_relevant_event(event: &Event) -> bool {
    use notify::EventKind;

    match &event.kind {
        EventKind::Access(_) => false, // Read access doesn't matter
        EventKind::Modify(modify_kind) => {
            use notify::event::ModifyKind;
            !matches!(
                modify_kind,
                ModifyKind::Metadata(_) // Metadata-only changes
            )
        }
        EventKind::Create(_) => true,
        EventKind::Remove(_) => true,
        _ => false,
    }
}
