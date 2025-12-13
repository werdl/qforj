use crate::defs::{CueStack};




pub fn serialize_cue_stack(cue_stack: &CueStack, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let toml_string = serde_yaml::to_string(cue_stack)?;
    std::fs::write(file_path, toml_string)?;
    Ok(())
}

pub fn deserialize_cue_stack(file_path: &str) -> Result<CueStack, Box<dyn std::error::Error>> {
    let toml_string = std::fs::read_to_string(file_path)?;
    let cue_stack: CueStack = serde_yaml::from_str(&toml_string)?;

    Ok(cue_stack)
}
