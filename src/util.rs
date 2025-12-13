pub fn db_to_normalized(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn is_zero_f32(val: &f32) -> bool {
    *val == 0.0
}

pub fn is_zero_u32(val: &u32) -> bool {
    *val == 0
}
