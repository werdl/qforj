use crate::defs::{Cue, CueStack};
use crate::error::Error;
use crate::util::db_to_normalized;
use crate::util::is_zero_u32;
use rodio::{Decoder, Sink, Source, mixer::Mixer};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::time::Duration;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub enum CueType {
    Audio(AudioCue),
    Stop(StopCue),
}

fn default_play() -> u32 {
    1
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AudioCue {
    /// path to the audio file
    pub path: String,

    /// start time within the audio file
    #[serde(
        with = "humantime_serde",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub start_time: Option<Duration>,

    /// end time within the audio file
    #[serde(
        with = "humantime_serde",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub end_time: Option<Duration>,

    /// amount of times to play the audio file (overriden by `loops` if `loops` is true)
    #[serde(default = "default_play", skip_serializing_if = "is_zero_u32")]
    pub play: u32,

    /// whether to loop the audio file indefinitely (overrides `play` if true)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub loops: bool,

    /// tracks whether the cue has been primed for playback
    #[serde(skip, default)]
    pub primed: bool,
}

pub struct AudioSink {
    pub sink: Sink,
    pub cue_number: f32,
}

impl std::fmt::Debug for AudioSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioSink")
            .field("cue_number", &self.cue_number)
            .finish()
    }
}

impl AudioCue {
    pub fn new(
        path: String,
        start_time: Option<Duration>,
        end_time: Option<Duration>,
        play: u32,
        loops: bool,
    ) -> Self {
        AudioCue {
            path,
            start_time,
            end_time,
            play,
            loops,
            primed: false,
        }
    }

    pub fn to_source(&self) -> impl Source<Item = f32> + Send + 'static {
        let file = File::open(&self.path).expect("Failed to open audio file");
        let decoder = Decoder::new(BufReader::new(file)).expect("Failed to decode audio file");

        let start = self.start_time.unwrap_or(Duration::from_secs(0));
        let skipped = decoder.skip_duration(start);

        // always wrap in take_duration so the return type is consistent
        let duration = match self.end_time {
            Some(end) => end.checked_sub(start).unwrap_or(Duration::from_secs(0)),
            None => Duration::from_secs(10_000_000), // effectively “until end of file”
        };

        skipped.take_duration(duration)
    }

    /// Prime the audio cue for playback by creating a Sink and loading the audio data
    pub fn prime(&self, mixer: &Mixer, cue_number: f32, trim: f32) -> AudioSink {
        let sink = Sink::connect_new(&mixer.clone());

        let source = self.to_source().amplify_decibel(trim);

        if self.loops {
            sink.append(source.repeat_infinite());
            sink.pause()
        } else {
            for _ in 0..self.play {
                sink.append(self.to_source());
                sink.pause();
            }
        }

        AudioSink { sink, cue_number }
    }
}

impl AudioSink {
    /// Start playback of the primed audio cue
    pub fn play(&mut self) {
        // self.set_volume(0.0); // default to 0 dB, approximately 0.316 normalized
        self.sink.play();
    }

    /// Stop playback of the primed audio cue
    pub fn stop(&mut self) {
        self.sink.stop();
    }

    /// Set the volume of the primed audio cue in decibels
    pub fn set_volume(&mut self, db: f32) {
        let volume = db_to_normalized(db);
        self.sink.set_volume(volume);
    }

    /// fade the volume to the target db over the specified duration
    pub fn fade_to(&mut self, target_db: f32, duration: Duration) {
        let target_volume = db_to_normalized(target_db);
        let current_volume = self.sink.volume();
        let steps = 100;
        let step_duration = duration / steps;
        let volume_step = (target_volume - current_volume) / steps as f32;

        for i in 0..steps {
            let new_volume = current_volume + volume_step * (i as f32 + 1.0);
            self.sink.set_volume(new_volume);
            std::thread::sleep(step_duration);
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct StopCue {
    pub target_cue: f32,
}

impl StopCue {
    pub fn go(&self, stack: &mut CueStack) -> Result<(), Error> {
        let sink = stack.get_sink_by_cue_number(self.target_cue);
        match sink {
            Some(sink) => {
                sink.stop();
                Ok(())
            }
            None => Err(Error::CueNotFound(self.target_cue)),
        }
    }
}
