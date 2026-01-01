use extism_pdk::*;
use nd_pdk::host::{library, scheduler, kvstore};
use nd_pdk::lifecycle::{Error as LifecycleError, InitProvider};
use nd_pdk::scheduler::{CallbackProvider, Error as SchedulerError, SchedulerCallbackRequest};
use bliss_audio::Song;
use serde_json;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::meta::MetadataOptions; 
use symphonia::core::audio::{SampleBuffer};
use symphonia::default::{get_probe};
use std::fs::File;

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

    // Check if analysis exists
	match kvstore::get(&key) {
		Ok((data, true)) => {
			let preview = match std::str::from_utf8(&data) {
				Ok(s) => &s[..s.len().min(20)],
				Err(_) => &hex::encode(&data)[..20], // hex, always safe
			};
			info!(
				"Bliss analysis already present for {}, skipping... First 20 chars: {}",
				file_path, preview
			);
			return;
		}
		Ok((_, false)) | Err(_) => { /* Key not found, fall through to analyze */ }
	}

    let decoded_samples = match decode_pcm_samples(file_path) {
		Ok(samples) => {
			info!("    Was able to decode file {}: {} samples", file_path, samples.len());
			samples
		},
		Err(e) => {
			error!("Failed to decode audio for {}: {}", file_path, e);
			return;
		}
	};

	if decoded_samples.is_empty() {
		error!("    PCM sample count for {}: {}", file_path, decoded_samples.len());
		return;
	}
	
	let analyze_result = std::panic::catch_unwind(|| Song::analyze(&decoded_samples));
	match analyze_result {
		Ok(song_result) => match song_result {
			Ok(analysis) => {
				match serde_json::to_vec(&analysis) {
					Ok(value) => {
						if let Err(e) = kvstore::set(&key, value) {
							error!("    Failed to store analysis for {}: {}", file_path, e);
						}
					}
					Err(e) => {
						error!(
							"    Failed to serialize analysis for {}: {:?} (type: {})",
							file_path, e, std::any::type_name::<Song>()
						);
					}
				}
			}
			Err(e) => {
				error!("    Bliss analysis failed for {}: {}", file_path, e);
			}
		},
		Err(_) => {
			error!("    Song::analyze panicked on file: {}", file_path);
		}
	}
	info!("    Song::analyze did not panic for {}", file_path);
}

//instead of ffmpeg depencecy (c-crosscompiled) use the symphonia pure rust decoder
// PCM decoder helper for most audio formats using Symphonia
fn decode_pcm_samples(file_path: &str) -> Result<Vec<f32>, String> {
    let file = File::open(file_path).map_err(|e| format!("Open error: {}", e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let probe = get_probe();
    let meta_opts = MetadataOptions::default();
    let probed = probe.format(&Default::default(), mss, &Default::default(), &meta_opts);
    let probed = match probed {
        Ok(p) => p,
        Err(e) => return Err(format!("    Symphonia error: {}", e)),
    };
    let mut format = probed.format;
    let track = format.default_track().ok_or("    No default track found")?;
    let codec_params = &track.codec_params;

    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .map_err(|e| format!("    Decoder error: {:?}", e))?;

    let mut pcm_data: Vec<f32> = Vec::new();

    loop {
        match format.next_packet() {
            Ok(packet) => {
                let decoded = decoder.decode(&packet).map_err(|e| format!("Decode error: {:?}", e))?;
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                buf.copy_interleaved_ref(decoded);
                pcm_data.extend_from_slice(buf.samples());
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                decoder.reset();
            }
            Err(symphonia::core::errors::Error::IoError(_)) => {
                break; // End of stream
            }
            Err(e) => {
                return Err(format!("    Packet error: {:?}", e));
            }
        }
    }
    Ok(pcm_data)
}

fn process_dir_recursively(dir: &str, counter: &mut usize, limit: usize) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                process_dir_recursively(&path.to_string_lossy(), counter, limit);
            } else if path.is_file() {
                if *counter < limit {
                    *counter += 1;
                    println!("    Analyzing file {}: {}", *counter, path.display());
                    analyze_and_store_if_missing(&path.to_string_lossy());
                } else {
                    return;
                }
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
			
			let mut counter = 0;
			let limit = 10; // TODO: make this a config item 
            process_dir_recursively(&lib.mount_point, &mut counter, limit);
        }
    }
}