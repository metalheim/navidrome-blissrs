//! Library Inspector Plugin for Navidrome
//!
//! This plugin demonstrates how to use the nd-pdk crate for accessing Navidrome
//! host services and implementing capabilities in Rust. It periodically logs details
//! about all music libraries and finds the largest file in the root of each library.
//!
//! ## Configuration
//!
//! Set the `cron` config key to customize the schedule (default: "@every 1m"):
//! ```toml
//! [PluginConfig.library-inspector]
//! cron = "@every 5m"
//! ```

use extism_pdk::*;
use nd_pdk::host::{library, scheduler};
use nd_pdk::lifecycle::{Error as LifecycleError, InitProvider};
use nd_pdk::scheduler::{CallbackProvider, Error as SchedulerError, SchedulerCallbackRequest};
use nd_pdk::host::{library, kv};
use bliss-rs; // Add actual path if needed
use serde_json;
use std::fs;

// Register capabilities using PDK macros
nd_pdk::register_lifecycle_init!(LibraryInspector);
nd_pdk::register_scheduler_callback!(LibraryInspector);

// ============================================================================
// Plugin Implementation
// ============================================================================

/// The library inspector plugin type.
#[derive(Default)]
struct LibraryInspector;

impl InitProvider for LibraryInspector {
    fn on_init(&self) -> Result<(), LifecycleError> {
        info!("bliss-rs plugin initializing...");

        // Get cron expression from config, default to every 24h
        let cron = config::get("cron")
            .ok()
            .flatten()
            .unwrap_or_else(|| "@every 24h".to_string());

        info!("Scheduling library inspection with cron: {}", cron);

        // Schedule the recurring task using nd-pdk host scheduler
        match scheduler::schedule_recurring(&cron, "inspect", "library-inspect") {
            Ok(schedule_id) => {
                info!("Scheduled inspection task with ID: {}", schedule_id);
            }
            Err(e) => {
                let error_msg = format!("Failed to schedule inspection: {}", e);
                error!("{}", error_msg);
                return Err(LifecycleError::new(error_msg));
            }
        }

        // Run an initial inspection
        inspect_libraries();

        info!("Library Inspector plugin initialized successfully");
        Ok(())
    }
}

impl CallbackProvider for LibraryInspector {
    fn on_callback(&self, req: SchedulerCallbackRequest) -> Result<(), SchedulerError> {
        info!(
            "Scheduler callback fired: schedule_id={}, payload={}, recurring={}",
            req.schedule_id, req.payload, req.is_recurring
        );

        if req.payload == "inspect" {
            inspect_libraries();
        }

        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn analyze_and_store_file(file_path: &str) {
    // Run bliss analysis
    match bliss::analyze(file_path) {
        Ok(analysis) => {
            let key = format!("bliss:{}", file_path); // Use a file path as key
            let value = serde_json::to_vec(&analysis).unwrap();
            if let Err(e) = kv::set(&key, &value) {
                error!("Failed to store analysis for {}: {}", file_path, e);
            }
        }
        Err(e) => error!("Bliss analysis failed for {}: {}", file_path, e),
    }
}

fn inspect_libraries() {
	let libraries = match library::get_all_libraries() {
		Ok(libs) => libs,
		Err(e) => {
			error!("Failed to get libraries: {}", e);
			return;
		}
    };

    if libraries.is_empty() {
        info!("No libraries configured");
        return;
    }

    info!("Found {} libraries, starting analysis", libraries.len());
	
    for lib in &libraries {
        info!("----------------------------------------");
        info!("Library: {} (ID: {})", lib.name, lib.id);
        info!("  Songs:    {} tracks", lib.total_songs);
		
        if !lib.mount_point.is_empty() {
            if let Ok(entries) = std::fs::read_dir(&lib.mount_point) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let path_str = path.to_string_lossy();
                        analyze_and_store_file(&path_str);
                    }
                }
            }
        }
    }
}