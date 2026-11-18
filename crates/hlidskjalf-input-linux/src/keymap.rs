//! Platform-neutral translation from Linux evdev key codes to Hliðskjálf wire key data.

use hlidskjalf_input::keycode;

/// Marks a PC set-1 scancode that is sent with the 0xE0 prefix.
pub const EXTENDED: u32 = 0xe000;

/// PC set-1 make code of the physical key, or `None` when it has no stable code.
pub fn scancode(code: u16) -> Option<u32> {
    let code = u32::from(code);
    if (1..=88).contains(&code) {
        // KEY_ESC through KEY_F12 share their numbering with set-1 make codes.
        return Some(code);
    }
    Some(match code {
        89 => 0x73,             // KEY_RO: ABNT2 /?
        96 => EXTENDED | 0x1c,  // KEY_KPENTER
        97 => EXTENDED | 0x1d,  // KEY_RIGHTCTRL
        98 => EXTENDED | 0x35,  // KEY_KPSLASH
        99 => EXTENDED | 0x37,  // KEY_SYSRQ
        100 => EXTENDED | 0x38, // KEY_RIGHTALT
        102 => EXTENDED | 0x47, // KEY_HOME
        103 => EXTENDED | 0x48, // KEY_UP
        104 => EXTENDED | 0x49, // KEY_PAGEUP
        105 => EXTENDED | 0x4b, // KEY_LEFT
        106 => EXTENDED | 0x4d, // KEY_RIGHT
        107 => EXTENDED | 0x4f, // KEY_END
        108 => EXTENDED | 0x50, // KEY_DOWN
        109 => EXTENDED | 0x51, // KEY_PAGEDOWN
        110 => EXTENDED | 0x52, // KEY_INSERT
        111 => EXTENDED | 0x53, // KEY_DELETE
        113 => EXTENDED | 0x20, // KEY_MUTE
        114 => EXTENDED | 0x2e, // KEY_VOLUMEDOWN
        115 => EXTENDED | 0x30, // KEY_VOLUMEUP
        121 => 0x7e,            // KEY_KPCOMMA: ABNT2 keypad separator
        124 => 0x7d,            // KEY_YEN
        125 => EXTENDED | 0x5b, // KEY_LEFTMETA
        126 => EXTENDED | 0x5c, // KEY_RIGHTMETA
        127 => EXTENDED | 0x5d, // KEY_COMPOSE (menu)
        163 => EXTENDED | 0x19, // KEY_NEXTSONG
        164 => EXTENDED | 0x22, // KEY_PLAYPAUSE
        165 => EXTENDED | 0x10, // KEY_PREVIOUSSONG
        _ => return None,
    })
}

const LETTERS: [(u16, u8); 26] = [
    (30, b'A'),
    (48, b'B'),
    (46, b'C'),
    (32, b'D'),
    (18, b'E'),
    (33, b'F'),
    (34, b'G'),
    (35, b'H'),
    (23, b'I'),
    (36, b'J'),
    (37, b'K'),
    (38, b'L'),
    (50, b'M'),
    (49, b'N'),
    (24, b'O'),
    (25, b'P'),
    (16, b'Q'),
    (19, b'R'),
    (31, b'S'),
    (20, b'T'),
    (22, b'U'),
    (47, b'V'),
    (17, b'W'),
    (45, b'X'),
    (21, b'Y'),
    (44, b'Z'),
];

/// The evdev code of the key at a letter's US QWERTY position.
pub fn letter_code(letter: char) -> Option<u16> {
    let letter = letter.to_ascii_uppercase();
    LETTERS
        .iter()
        .find(|(_, candidate)| char::from(*candidate) == letter)
        .map(|(code, _)| *code)
}

/// Best-effort layout-independent wire key code, used by receivers without scancode support.
pub fn wire_code(code: u16) -> u32 {
    if let Some((_, letter)) = LETTERS.iter().find(|(candidate, _)| *candidate == code) {
        return u32::from(*letter);
    }
    match code {
        2..=10 => u32::from(code) + 47, // KEY_1..KEY_9 -> '1'..'9'
        11 => 48,
        12 => 45,
        13 => 61,
        26 => 91,
        27 => 93,
        39 => 59,
        40 => 39,
        41 => 96,
        43 => 92,
        51 => 44,
        52 => 46,
        53 => 47,
        1 => keycode::ESCAPE,
        14 => keycode::BACKSPACE,
        15 => keycode::TAB,
        28 | 96 => keycode::ENTER,
        29 => keycode::LEFT_CONTROL,
        97 => keycode::RIGHT_CONTROL,
        42 => keycode::LEFT_SHIFT,
        54 => keycode::RIGHT_SHIFT,
        56 => keycode::LEFT_ALT,
        100 => keycode::RIGHT_ALT,
        57 => keycode::SPACE,
        58 => keycode::CAPS_LOCK,
        59..=68 => keycode::F1 + u32::from(code - 59),
        87 => keycode::F1 + 10,
        88 => keycode::F1 + 11,
        102 => keycode::HOME,
        103 => keycode::UP,
        104 => keycode::PAGE_UP,
        105 => keycode::LEFT,
        106 => keycode::RIGHT,
        107 => keycode::END,
        108 => keycode::DOWN,
        109 => keycode::PAGE_DOWN,
        110 => keycode::INSERT,
        111 => keycode::DELETE,
        125 => keycode::LEFT_META,
        126 => keycode::RIGHT_META,
        _ => 0,
    }
}

/// Modifier bit for the return shortcut: 1 Ctrl, 2 Shift, 4 Alt.
pub fn modifier_bit(code: u16) -> u8 {
    match code {
        29 | 97 => 1,
        42 | 54 => 2,
        56 | 100 => 4,
        _ => 0,
    }
}

/// Parses a shortcut such as `ctrl+shift+Q` into modifier bits and an evdev key code.
pub fn parse_shortcut(value: &str) -> Option<(u8, u16)> {
    let mut parts: Vec<&str> = value.split('+').collect();
    let key = parts.pop()?;
    let mut modifiers = 0_u8;
    for part in parts {
        modifiers |= match part.to_ascii_lowercase().as_str() {
            "ctrl" => 1,
            "shift" => 2,
            "alt" => 4,
            _ => return None,
        };
    }
    if modifiers.count_ones() < 2 {
        return None;
    }
    let mut characters = key.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    let code = if character.is_ascii_digit() {
        match character {
            '0' => 11,
            digit => digit as u16 - '1' as u16 + 2,
        }
    } else {
        letter_code(character)?
    };
    Some((modifiers, code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_physical_keys_to_set_one_scancodes() {
        assert_eq!(scancode(30), Some(0x1e));
        assert_eq!(scancode(86), Some(0x56));
        assert_eq!(scancode(89), Some(0x73));
        assert_eq!(scancode(100), Some(EXTENDED | 0x38));
        assert_eq!(scancode(105), Some(EXTENDED | 0x4b));
        assert_eq!(scancode(0x110), None);
    }

    #[test]
    fn maps_wire_codes_for_receivers_without_scancodes() {
        assert_eq!(wire_code(30), u32::from(b'A'));
        assert_eq!(wire_code(2), u32::from(b'1'));
        assert_eq!(wire_code(11), u32::from(b'0'));
        assert_eq!(wire_code(59), keycode::F1);
        assert_eq!(wire_code(88), keycode::F1 + 11);
        assert_eq!(wire_code(103), keycode::UP);
    }

    #[test]
    fn parses_return_shortcuts() {
        assert_eq!(parse_shortcut("ctrl+shift+Q"), Some((3, 16)));
        assert_eq!(parse_shortcut("ctrl+alt+0"), Some((5, 11)));
        assert_eq!(parse_shortcut("ctrl+Q"), None);
        assert_eq!(parse_shortcut("ctrl+shift+F1"), None);
    }
}
