//! Platform-neutral translation from Hliðskjálf wire key codes to Windows virtual keys.

use hlidskjalf_input::keycode;

/// A Windows virtual key plus whether it belongs to the extended key set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualKey {
    pub vk: u16,
    pub extended: bool,
}

const fn key(vk: u16) -> Option<VirtualKey> {
    Some(VirtualKey {
        vk,
        extended: false,
    })
}

const fn extended(vk: u16) -> Option<VirtualKey> {
    Some(VirtualKey { vk, extended: true })
}

/// Maps a non-Unicode wire key code to a Windows virtual key.
pub fn virtual_key(code: u32) -> Option<VirtualKey> {
    if (65..=90).contains(&code) || (48..=57).contains(&code) {
        return key(code as u16);
    }
    if (keycode::F1..=keycode::F1 + 11).contains(&code) {
        return key(0x70 + (code - keycode::F1) as u16);
    }
    match code {
        59 => key(0xba),
        61 => key(0xbb),
        44 => key(0xbc),
        45 => key(0xbd),
        46 => key(0xbe),
        47 => key(0xbf),
        96 => key(0xc0),
        91 => key(0xdb),
        92 => key(0xdc),
        93 => key(0xdd),
        39 => key(0xde),
        keycode::ENTER => key(0x0d),
        keycode::ESCAPE => key(0x1b),
        keycode::BACKSPACE => key(0x08),
        keycode::TAB => key(0x09),
        keycode::SPACE => key(0x20),
        keycode::LEFT => extended(0x25),
        keycode::RIGHT => extended(0x27),
        keycode::UP => extended(0x26),
        keycode::DOWN => extended(0x28),
        keycode::DELETE => extended(0x2e),
        keycode::LEFT_SHIFT => key(0xa0),
        keycode::RIGHT_SHIFT => key(0xa1),
        keycode::LEFT_CONTROL => key(0xa2),
        keycode::RIGHT_CONTROL => extended(0xa3),
        keycode::LEFT_ALT => key(0xa4),
        keycode::RIGHT_ALT => extended(0xa5),
        keycode::LEFT_META => extended(0x5b),
        keycode::RIGHT_META => extended(0x5c),
        keycode::HOME => extended(0x24),
        keycode::END => extended(0x23),
        keycode::PAGE_UP => extended(0x21),
        keycode::PAGE_DOWN => extended(0x22),
        keycode::INSERT => extended(0x2d),
        keycode::CAPS_LOCK => key(0x14),
        _ => None,
    }
}

/// Returns the Unicode character carried by a translated wire key code.
pub fn unicode_character(code: u32) -> Option<char> {
    code.checked_sub(keycode::UNICODE_BASE)
        .filter(|value| *value <= 0x10ffff)
        .and_then(char::from_u32)
}

/// Returns the X11 dead keysym (0xfe50 through 0xfe57) carried by a wire key code.
pub fn dead_key(code: u32) -> Option<u32> {
    code.checked_sub(keycode::DEAD_BASE)
        .filter(|keysym| (0xfe50..=0xfe57).contains(keysym))
}

/// The spacing character typed when a dead key is followed by a key it cannot combine with.
pub fn dead_key_spacing(keysym: u32) -> Option<char> {
    Some(match keysym {
        0xfe50 => '`',
        0xfe51 => '´',
        0xfe52 => '^',
        0xfe53 => '~',
        0xfe57 => '¨',
        _ => return None,
    })
}

/// Combines a dead key with a base character, as a Windows layout would.
pub fn compose(keysym: u32, base: char) -> Option<char> {
    const TABLE: [(char, [char; 5]); 14] = [
        // base, [grave, acute, circumflex, tilde, diaeresis]; '\0' means no composition.
        ('a', ['à', 'á', 'â', 'ã', 'ä']),
        ('e', ['è', 'é', 'ê', 'ẽ', 'ë']),
        ('i', ['ì', 'í', 'î', 'ĩ', 'ï']),
        ('o', ['ò', 'ó', 'ô', 'õ', 'ö']),
        ('u', ['ù', 'ú', 'û', 'ũ', 'ü']),
        ('y', ['ỳ', 'ý', 'ŷ', 'ỹ', 'ÿ']),
        ('n', ['ǹ', 'ń', '\0', 'ñ', '\0']),
        ('c', ['\0', 'ć', 'ĉ', '\0', '\0']),
        ('A', ['À', 'Á', 'Â', 'Ã', 'Ä']),
        ('E', ['È', 'É', 'Ê', 'Ẽ', 'Ë']),
        ('I', ['Ì', 'Í', 'Î', 'Ĩ', 'Ï']),
        ('O', ['Ò', 'Ó', 'Ô', 'Õ', 'Ö']),
        ('U', ['Ù', 'Ú', 'Û', 'Ũ', 'Ü']),
        ('N', ['Ǹ', 'Ń', '\0', 'Ñ', '\0']),
    ];
    let column = match keysym {
        0xfe50 => 0,
        0xfe51 => 1,
        0xfe52 => 2,
        0xfe53 => 3,
        0xfe57 => 4,
        _ => return None,
    };
    if base == ' ' {
        return dead_key_spacing(keysym);
    }
    TABLE
        .iter()
        .find(|(candidate, _)| *candidate == base)
        .map(|(_, row)| row[column])
        .filter(|composed| *composed != '\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_navigation_and_function_keys() {
        assert_eq!(virtual_key(65), key(0x41));
        assert_eq!(virtual_key(keycode::LEFT), extended(0x25));
        assert_eq!(virtual_key(keycode::F1 + 11), key(0x7b));
        assert_eq!(virtual_key(keycode::RIGHT_ALT), extended(0xa5));
        assert_eq!(virtual_key(keycode::F1 + 12), None);
        assert_eq!(virtual_key(keycode::UNICODE_BASE + 'a' as u32), None);
    }

    #[test]
    fn decodes_unicode_and_dead_keys() {
        assert_eq!(
            unicode_character(keycode::UNICODE_BASE + 'ç' as u32),
            Some('ç')
        );
        assert_eq!(unicode_character(65), None);
        assert_eq!(dead_key(keycode::DEAD_BASE + 0xfe51), Some(0xfe51));
        assert_eq!(dead_key(keycode::DEAD_BASE + 0xfe58), None);
    }

    #[test]
    fn composes_portuguese_accents() {
        assert_eq!(compose(0xfe51, 'a'), Some('á'));
        assert_eq!(compose(0xfe53, 'O'), Some('Õ'));
        assert_eq!(compose(0xfe52, 'e'), Some('ê'));
        assert_eq!(compose(0xfe50, 'a'), Some('à'));
        assert_eq!(compose(0xfe53, ' '), Some('~'));
        assert_eq!(compose(0xfe53, 'x'), None);
        assert_eq!(compose(0xfe52, 'n'), None);
    }
}
