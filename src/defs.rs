use rodio::mixer::Mixer;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cue_types::{AudioSink, CueType};
use crate::error::Error;

use log::{debug, error, trace};

/// continue mode determines when the next cue is triggered
#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum ContinueModes {
    /// next cue is triggered manually
    #[default]
    DoNotContinue,
    /// as the cue ends, the next cue is triggered automatically
    AutoFollow,
    /// once this cue starts, the next cue is triggered automatically
    AutoContinue,
}

/// a cue represents a single logical operation of the audio system
///
/// for example, a fade, a track, stopping playback, etc.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    /// cue number, can be non-integer
    pub number: f32,

    /// cue name
    pub name: String,

    /// wait `pre_wait` after the cue is called to actually start the operation
    #[serde(
        with = "humantime_serde",
        default,
        skip_serializing_if = "std::time::Duration::is_zero"
    )]
    pub pre_wait: Duration,

    /// wait `post_wait` after the operation is complete before triggering the next cue if
    /// applicable
    #[serde(
        with = "humantime_serde",
        default,
        skip_serializing_if = "std::time::Duration::is_zero"
    )]
    pub post_wait: Duration,

    /// continue mode for this cue
    #[serde(default)]
    pub continue_mode: ContinueModes,

    /// CueType containing operation-specific parameters, eg. fade duration, audio file path, etc.
    pub cue_type: CueType,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct CueStack {
    /// list of cues in this stack
    pub cues: Vec<Cue>,

    /// name of this cue stack
    pub name: String,

    /// continue mode for this cue stack
    #[serde(default)]
    pub continue_mode: ContinueModes,

    /// cue that is currently active (ie. the last one that was triggered)
    #[serde(skip, default)]
    pub current_cue_number: f32,

    /// list of primed audio sinks for this cue stack
    #[serde(skip, default)]
    pub audio_sinks: Vec<AudioSink>,
}

#[derive(PartialEq)]
pub enum State {
    Unfinished,
    Finished,
}

impl CueStack {
    pub fn get_cue_by_number(&self, number: f32) -> Option<&Cue> {
        self.cues.iter().find(|c| c.number == number)
    }

    pub fn get_cue_by_number_mut(&mut self, number: f32) -> Option<&mut Cue> {
        self.cues.iter_mut().find(|c| c.number == number)
    }

    pub fn get_sink_by_cue_number(&mut self, cue_number: f32) -> Option<&mut AudioSink> {
        self.audio_sinks
            .iter_mut()
            .find(|s| s.cue_number == cue_number)
    }

    pub fn prime_cue(&mut self, cue_number: f32, mixer: &Mixer) -> Result<(), Error> {
        trace!("priming cue {}", cue_number);
        // find the cue in the stack
        let cue_opt = self.get_cue_by_number_mut(cue_number);
        if let Some(cue) = cue_opt {
            match &mut cue.cue_type {
                CueType::Audio(audio_cue) => {
                    let primed = audio_cue.prime(mixer, cue_number, audio_cue.trim);
                    audio_cue.primed = true;
                    self.audio_sinks.push(primed);
                    debug!("primed audio cue {}", cue_number);
                    Ok(())
                }
                _ => Ok(()),
            }
        } else {
            Err(Error::CueNotFound(cue_number))
        }
    }

    pub fn prune_sinks(&mut self) {
        trace!("pruning sinks");
        // remove any sinks that have finished playing, and mark their cues as unprimed
        for i in (0..self.audio_sinks.len()).rev() {
            if self.audio_sinks[i].sink.empty() {
                // mark cue as unprimed
                if let Some(cue) = self.get_cue_by_number_mut(self.audio_sinks[i].cue_number) {
                    if let CueType::Audio(audio_cue) = &mut cue.cue_type {
                        audio_cue.primed = false;
                    }
                    trace!("pruned sink for cue {}", self.audio_sinks[i].cue_number);
                }
                self.audio_sinks.remove(i);
            }
        }
    }

    pub fn trigger_cue(&mut self, cue_number: f32) -> Result<(), Error> {
        debug!("triggering cue {}", cue_number);
        self.prune_sinks();
        // find the cue in the stack
        let cue_type = match self.get_cue_by_number(cue_number) {
            Some(cue) => cue.cue_type.clone(),
            None => return Err(Error::CueNotFound(cue_number)),
        };

        self.current_cue_number = cue_number;

        let res = match cue_type {
            CueType::Audio(_audio_cue) => {
                let sink_opt = self.get_sink_by_cue_number(cue_number);
                if let Some(sink) = sink_opt {
                    sink.play();
                    Ok(())
                } else {
                    error!("cue {} not primed", cue_number);
                    Err(Error::CueNotPrimed(cue_number))
                }
            }
            CueType::Stop(stop_cue) => stop_cue.go(self),
            CueType::Fade(fade_cue) => fade_cue.go(self),
        };
        self.prune_sinks();
        trace!("triggered cue {}", cue_number);
        res
    }

    pub fn go(&mut self) -> Result<State, Error> {
        trace!("going to next cue from {}", self.current_cue_number);
        // get next cue
        let prev_cue = self.get_cue_by_number(self.current_cue_number);

        let next_cue_index = match prev_cue {
            Some(cue) => {
                let index = self
                    .cues
                    .iter()
                    .position(|c| c.number == cue.number)
                    .unwrap();
                index + 1
            }
            None => 0,
        };

        if next_cue_index >= self.cues.len() {
            return Ok(State::Finished);
        }

        let new_cue = self.cues[next_cue_index].clone();

        trace!("next cue is {}", new_cue.number);

        // trigger next cue first
        self.trigger_cue(new_cue.number)?;

        // check continue mode using the cloned cue
        if new_cue.continue_mode == ContinueModes::AutoContinue
            || self.continue_mode == ContinueModes::AutoContinue
        {
            let next_cue_number = self.cues[next_cue_index].number;
            self.trigger_cue(next_cue_number)?;
        }

        Ok(State::Unfinished)
    }
}
