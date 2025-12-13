
#[derive(Debug)]
pub enum Error {
    CueNotFound(f32),
    CueNotPrimed(f32),
}
