use crate::defs::CueStack;
use crate::error::Error;
use crate::util::db_to_normalized;
use crate::util::is_zero_u32;
use rodio::{Decoder, Sink, Source, mixer::Mixer};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::time::Duration;
use log::trace;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
/// different types of cues with type-specific parameters
pub enum CueType {
    /// play an audio file
    Audio(AudioCue),
    /// stop playback of a cue
    Stop(StopCue),
    /// fade volume of a cue
    Fade(FadeCue)
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
    
    /// volume trim in decibels
    #[serde(default)]
    pub trim: f32,
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
        trim: f32,
    ) -> Self {
        AudioCue {
            path,
            start_time,
            end_time,
            play,
            loops,
            primed: false,
            trim,
        }
    }

    pub fn to_source(&self) -> impl Source<Item = f32> + Send + 'static {
        trace!("loading audio file: {}", &self.path);
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
        trace!("priming audio cue {}: {}", cue_number, &self.path);
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
    pub fn fade_to_linear(&mut self, target_db: f32, duration: Duration) {
        trace!("fading to {} dB over {:?}", target_db, duration);
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

    /// fade the volume to the target db over the specified duration exponentially (starts slow,
    /// ends fast)
    pub fn fade_to_exponential(&mut self, target_db: f32, duration: Duration) {
        trace!("fading to {} dB over {:?}", target_db, duration);
        let target_volume = db_to_normalized(target_db);
        let current_volume = self.sink.volume();
        let steps = 100;
        let step_duration = duration / steps;

        for i in 0..steps {
            let t = (i as f32 + 1.0) / steps as f32;
            let new_volume = current_volume * (target_volume / current_volume).powf(t);
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
        trace!("stopping cue {}", self.target_cue);
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

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub enum FadeShape {
    #[default]
    Linear,
    Exponential,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
/// fade cue parameters
pub struct FadeCue {
    /// target cue number to apply the fade to (ie. which audio cue)
    pub target_cue: f32,
    /// duration of the fade
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
    /// target volume in decibels
    pub target_db: f32,
    #[serde(default)]
    /// shape of the fade
    pub shape: FadeShape,
    #[serde(default)]
    /// whether to stop the cue after fading
    pub and_stop: bool,
}

impl FadeCue {
    pub fn go(&self, stack: &mut CueStack) -> Result<(), Error> {
        trace!("fading cue {} to {} dB over {:?}", self.target_cue, self.target_db, self.duration);
        let sink_opt = stack.get_sink_by_cue_number(self.target_cue);
        match sink_opt {
            Some(sink) => {
                match self.shape {
                    FadeShape::Linear => {
                        sink.fade_to_linear(self.target_db, self.duration);
                    }
                    FadeShape::Exponential => {
                        sink.fade_to_exponential(self.target_db, self.duration);
                    }
                }
                if self.and_stop {
                    sink.stop();
                }
                Ok(())
            }
            None => Err(Error::CueNotFound(self.target_cue)),
        }
    }
}
