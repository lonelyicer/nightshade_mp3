pub const CHARSET: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz{¦}~¢£¥¿¡µ÷°·•!\"#$%&'()*+,-./:;<=>?@[\\]^_` ";
pub const SPACE_ID: i32 = 104;

pub fn char_to_id(character: char) -> i32 {
    CHARSET
        .chars()
        .position(|candidate| candidate == character)
        .map(|index| index as i32)
        .unwrap_or(SPACE_ID)
}

pub fn encode_fixed(text: &str, width: usize) -> Vec<i32> {
    let mut output = text.chars().take(width).map(char_to_id).collect::<Vec<_>>();

    output.resize(width, SPACE_ID);

    output
}

pub fn supported_text(text: &str) -> String {
    text.chars()
        .map(|character| {
            if CHARSET.contains(character) {
                character
            } else {
                ' '
            }
        })
        .collect()
}
