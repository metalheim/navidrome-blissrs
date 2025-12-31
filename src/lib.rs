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
use bliss-audio;
use serde_json;
use std::fs;use walkdir::WalkDir;

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
        info!("bliss audio analysis plugin initializing...");

        // Get cron expression from config, default to every 24h
        let cron = config::get("cron")
            .ok()
            .flatten()
            .unwrap_or_else(|| "@every 24h".to_string());

        info!("Scheduling bliss audio analysis with cron: {}", cron);

        // Schedule the recurring task using nd-pdk host scheduler
        match scheduler::schedule_recurring(&cron, "inspect", "library-inspect") {
            Ok(schedule_id) => {
                info!("Scheduled inspection task with ID: {}", schedule_id);
            }
            Err(e) => {
                let error_msg = format!("Failed to schedule bliss audio analysis: {}", e);
                error!("{}", error_msg);
                return Err(LifecycleError::new(error_msg));
            }
        }

        // Run an initial inspection
        inspect_libraries();

        info!("bliss audio analysis plugin initialized successfully");
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

fn analyze_and_store_if_missing(file_path: &str) {
    let key = format!("bliss:{}", file_path);
	
	//TODO: implement a way to purge data for updated files (or have data expire after x time)
    // Check if analysis exists
    if let Ok(Some(_data)) = kv::get(&key) {
        // Analysis already exists, skip this file
        info!("Bliss analysis already present for {}, skipping...", file_path);
        return;
    }

    // Run bliss analysis if not cached
    match bliss::analyze(file_path) {
        Ok(analysis) => {
            let value = serde_json::to_vec(&analysis).unwrap();
            if let Err(e) = kv::set(&key, &value) {
                error!("Failed to store analysis for {}: {}", file_path, e);
            }
        }
        Err(e) => error!("Bliss analysis failed for {}: {}", file_path, e),
    }
}

fn process_dir_recursively(dir: &str) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                process_dir_recursively(&path.to_string_lossy());
            } else if path.is_file() {
                analyze_and_store_if_missing(&path.to_string_lossy());
            }
        }
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
            process_dir_recursively(&lib.mount_point);
        }
    }
}