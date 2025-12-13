use defs::{Cue, CueStack};
use rodio::Sink;
use rodio::source::{SineWave, Source};
use rodio::stream::OutputStreamBuilder;
use serialize::serialize_cue_stack;
use std::f32::NEG_INFINITY;
use std::time::Duration;

mod cue_types;
mod defs;
mod error;
mod serialize;
mod util;

use cue_types::AudioCue;
use std::io::{self, Write};

fn main() {
    // Create a builder for the default device
    let builder =
        OutputStreamBuilder::from_default_device().expect("No default audio device found");

    // Open the audio stream
    let stream = builder.open_stream().expect("Failed to open audio stream");

    let mut stack = serialize::deserialize_cue_stack("cue_stack.yaml")
        .expect("Failed to deserialize cue stack");

    println!("loading audio data...");

    for cue in &mut stack.cues {
        if let cue_types::CueType::Audio(audio_cue) = &mut cue.cue_type {
            stack
                .audio_sinks
                .push(audio_cue.prime(stream.mixer(), cue.number, cue.trim));
            audio_cue.primed = true;
            println!("primed cue {}", cue.number);
        }
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
                    match stack.get_cue_by_number(cue_number) {
                        Some(c) => match &c.cue_type {
                            cue_types::CueType::Audio(a) => {
                                if !a.primed {
                                    stack.prime_cue(cue_number, stream.mixer()).unwrap_or_else(
                                        |e| eprintln!("Error priming cue: {:?}", e),
                                    );
                                }
                            }
                            _ => { /* do nothing for non-audio cues */ }
                        },
                        None => {
                            eprintln!("Cue number {} not found.", cue_number);
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
