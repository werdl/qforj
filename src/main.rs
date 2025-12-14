#![allow(clippy::uninlined_format_args)]

use rodio::stream::OutputStreamBuilder;


mod cue_types;
mod defs;
mod error;
mod serialize;
mod util;

use env_logger::Builder;
use std::env;
use log::{info, debug, LevelFilter};
use std::io::{self, Write};

fn main() {
    let mut builder = Builder::new();

    // Default global level
    builder.filter_level(LevelFilter::Info);

    builder.filter_module("symphonia_core", LevelFilter::Warn);
    builder.filter_module("symphonia_bundle_mp3", LevelFilter::Warn);

    if let Ok(rust_log) = env::var("RUST_LOG") {
        builder.parse_filters(&rust_log);
    }

    builder.init();

    // Create a builder for the default device
    let builder =
        OutputStreamBuilder::from_default_device().expect("No default audio device found");

    // Open the audio stream
    let stream = builder.open_stream().expect("Failed to open audio stream");

    let mut stack = serialize::deserialize_cue_stack("cue_stack.yaml")
        .expect("Failed to deserialize cue stack");

    info!("loading audio data...");

    for cue in &stack.cues.clone() {
        stack
            .prime_cue(cue.number, stream.mixer())
            .unwrap_or_else(|e| {
                eprintln!("Error priming cue {}: {:?}", cue.number, e);
            });
        debug!("primed cue {}", cue.number);
    }

    loop {
        let mut input = String::new();

        print!("> ");
        io::stdout().flush().unwrap(); // ensure the prompt prints

        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");

        let trimmed = input.trim();
        match trimmed {
            "exit" => break,
            "go" => {
                if let Err(e) = stack.go() {
                    eprintln!("Error: {:?}", e);
                }
            }
            _ => match trimmed.parse::<f32>() {
                Ok(cue_number) => {
                    let c = stack.get_cue_by_number(cue_number);
                    match c {
                        Some(_) => {
                            stack.prune_sinks();
                            if let Err(e) = stack.prime_cue(cue_number, stream.mixer()) {
                                eprintln!("Error priming cue {}: {:?}", cue_number, e);
                                continue;
                            }
                        }
                        None => {
                            eprintln!("Cue {} not found in stack.", cue_number);
                            continue;
                        }
                    }

                    if let Err(e) = stack.trigger_cue(cue_number) {
                        eprintln!("Error: {:?}", e);
                    }
                }
                Err(_) => {
                    eprintln!(
                        "go - trigger next cue
<n> - prime and trigger cue number n
exit - exit the program"
                    );
                }
            },
        }
    }
}
