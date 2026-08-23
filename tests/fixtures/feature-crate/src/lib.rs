pub fn always(value: i32) -> i32 {
    value
}

#[cfg(feature = "extra")]
pub fn feature_alpha(value: i32) -> i32 {
    let adjusted = value + 1;
    if adjusted > 4 {
        adjusted * 2
    } else {
        adjusted - 1
    }
}

#[cfg(feature = "extra")]
pub fn feature_beta(input: i32) -> i32 {
    let changed = input + 9;
    if changed > 8 {
        changed * 7
    } else {
        changed - 3
    }
}
