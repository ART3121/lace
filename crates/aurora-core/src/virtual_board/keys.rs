//! As teclas do computador como um teclado PS/2 as manda: o código de
//! varredura do conjunto 2 de cada tecla, com `E0` antes nas estendidas e
//! `F0` antes do código quando a tecla é solta. É o que o `myKeyPS2` da
//! biblioteca SAPHO e os controladores de teclado das placas leem.

/// O código de varredura (conjunto 2) da tecla e se ela é estendida (`E0`),
/// pelo `KeyboardEvent.code` dos navegadores.
fn scancode(code: &str) -> Option<(u8, bool)> {
    let plain = |c: u8| Some((c, false));
    let extended = |c: u8| Some((c, true));
    match code {
        "KeyA" => plain(0x1C),
        "KeyB" => plain(0x32),
        "KeyC" => plain(0x21),
        "KeyD" => plain(0x23),
        "KeyE" => plain(0x24),
        "KeyF" => plain(0x2B),
        "KeyG" => plain(0x34),
        "KeyH" => plain(0x33),
        "KeyI" => plain(0x43),
        "KeyJ" => plain(0x3B),
        "KeyK" => plain(0x42),
        "KeyL" => plain(0x4B),
        "KeyM" => plain(0x3A),
        "KeyN" => plain(0x31),
        "KeyO" => plain(0x44),
        "KeyP" => plain(0x4D),
        "KeyQ" => plain(0x15),
        "KeyR" => plain(0x2D),
        "KeyS" => plain(0x1B),
        "KeyT" => plain(0x2C),
        "KeyU" => plain(0x3C),
        "KeyV" => plain(0x2A),
        "KeyW" => plain(0x1D),
        "KeyX" => plain(0x22),
        "KeyY" => plain(0x35),
        "KeyZ" => plain(0x1A),
        "Digit0" => plain(0x45),
        "Digit1" => plain(0x16),
        "Digit2" => plain(0x1E),
        "Digit3" => plain(0x26),
        "Digit4" => plain(0x25),
        "Digit5" => plain(0x2E),
        "Digit6" => plain(0x36),
        "Digit7" => plain(0x3D),
        "Digit8" => plain(0x3E),
        "Digit9" => plain(0x46),
        "Backquote" => plain(0x0E),
        "Minus" => plain(0x4E),
        "Equal" => plain(0x55),
        "Backslash" => plain(0x5D),
        "BracketLeft" => plain(0x54),
        "BracketRight" => plain(0x5B),
        "Semicolon" => plain(0x4C),
        "Quote" => plain(0x52),
        "Comma" => plain(0x41),
        "Period" => plain(0x49),
        "Slash" => plain(0x4A),
        // A tecla a mais do ABNT2 e do ISO: `\|` ao lado do Shift esquerdo
        // e `/?` ao lado do Shift direito.
        "IntlBackslash" => plain(0x61),
        "IntlRo" => plain(0x51),
        "Space" => plain(0x29),
        "Backspace" => plain(0x66),
        "Tab" => plain(0x0D),
        "Enter" => plain(0x5A),
        "Escape" => plain(0x76),
        "CapsLock" => plain(0x58),
        "ShiftLeft" => plain(0x12),
        "ShiftRight" => plain(0x59),
        "ControlLeft" => plain(0x14),
        "ControlRight" => extended(0x14),
        "AltLeft" => plain(0x11),
        "AltRight" => extended(0x11),
        "MetaLeft" => extended(0x1F),
        "MetaRight" => extended(0x27),
        "ContextMenu" => extended(0x2F),
        "F1" => plain(0x05),
        "F2" => plain(0x06),
        "F3" => plain(0x04),
        "F4" => plain(0x0C),
        "F5" => plain(0x03),
        "F6" => plain(0x0B),
        "F7" => plain(0x83),
        "F8" => plain(0x0A),
        "F9" => plain(0x01),
        "F10" => plain(0x09),
        "F11" => plain(0x78),
        "F12" => plain(0x07),
        "ScrollLock" => plain(0x7E),
        "NumLock" => plain(0x77),
        "Numpad0" => plain(0x70),
        "Numpad1" => plain(0x69),
        "Numpad2" => plain(0x72),
        "Numpad3" => plain(0x7A),
        "Numpad4" => plain(0x6B),
        "Numpad5" => plain(0x73),
        "Numpad6" => plain(0x74),
        "Numpad7" => plain(0x6C),
        "Numpad8" => plain(0x75),
        "Numpad9" => plain(0x7D),
        "NumpadDecimal" => plain(0x71),
        "NumpadComma" => plain(0x6D),
        "NumpadAdd" => plain(0x79),
        "NumpadSubtract" => plain(0x7B),
        "NumpadMultiply" => plain(0x7C),
        "NumpadDivide" => extended(0x4A),
        "NumpadEnter" => extended(0x5A),
        "Insert" => extended(0x70),
        "Delete" => extended(0x71),
        "Home" => extended(0x6C),
        "End" => extended(0x69),
        "PageUp" => extended(0x7D),
        "PageDown" => extended(0x7A),
        "ArrowUp" => extended(0x75),
        "ArrowDown" => extended(0x72),
        "ArrowLeft" => extended(0x6B),
        "ArrowRight" => extended(0x74),
        _ => None,
    }
}

/// Os bytes que o teclado manda quando a tecla é apertada ou solta; `None`
/// para uma tecla que o conjunto 2 não tem (ou que a tabela não conhece).
pub(super) fn key_bytes(code: &str, pressed: bool) -> Option<Vec<u8>> {
    let (scan, extended) = scancode(code)?;
    let mut bytes = Vec::with_capacity(3);
    if extended {
        bytes.push(0xE0);
    }
    if !pressed {
        bytes.push(0xF0);
    }
    bytes.push(scan);
    Some(bytes)
}

/// As teclas que digitam o caractere num teclado americano, e se precisa
/// de Shift.
fn keys_for(c: char) -> Option<(&'static str, bool)> {
    const LETTERS: [&str; 26] = [
        "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK",
        "KeyL", "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV",
        "KeyW", "KeyX", "KeyY", "KeyZ",
    ];
    const DIGITS: [&str; 10] = [
        "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8",
        "Digit9",
    ];
    // Os símbolos com Shift nos números, de 0 a 9.
    const SHIFTED_DIGITS: &str = ")!@#$%^&*(";
    if c.is_ascii_lowercase() {
        return Some((LETTERS[(c as u8 - b'a') as usize], false));
    }
    if c.is_ascii_uppercase() {
        return Some((LETTERS[(c as u8 - b'A') as usize], true));
    }
    if c.is_ascii_digit() {
        return Some((DIGITS[(c as u8 - b'0') as usize], false));
    }
    if let Some(at) = SHIFTED_DIGITS.find(c) {
        return Some((DIGITS[at], true));
    }
    let key = match c {
        ' ' => ("Space", false),
        '\n' | '\r' => ("Enter", false),
        '\t' => ("Tab", false),
        '\u{8}' => ("Backspace", false),
        '\u{1b}' => ("Escape", false),
        '-' => ("Minus", false),
        '_' => ("Minus", true),
        '=' => ("Equal", false),
        '+' => ("Equal", true),
        '[' => ("BracketLeft", false),
        '{' => ("BracketLeft", true),
        ']' => ("BracketRight", false),
        '}' => ("BracketRight", true),
        ';' => ("Semicolon", false),
        ':' => ("Semicolon", true),
        '\'' => ("Quote", false),
        '"' => ("Quote", true),
        ',' => ("Comma", false),
        '<' => ("Comma", true),
        '.' => ("Period", false),
        '>' => ("Period", true),
        '/' => ("Slash", false),
        '?' => ("Slash", true),
        '`' => ("Backquote", false),
        '~' => ("Backquote", true),
        '\\' => ("Backslash", false),
        '|' => ("Backslash", true),
        _ => return None,
    };
    Some(key)
}

/// Os bytes que um teclado americano manda para digitar `text`: cada
/// caractere aperta e solta as teclas dele (com o Shift em volta, quando
/// precisa). Caracteres sem tecla ficam de fora.
pub(super) fn text_bytes(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for c in text.chars() {
        let Some((code, shift)) = keys_for(c) else {
            continue;
        };
        if shift {
            bytes.extend(key_bytes("ShiftLeft", true).unwrap_or_default());
        }
        bytes.extend(key_bytes(code, true).unwrap_or_default());
        bytes.extend(key_bytes(code, false).unwrap_or_default());
        if shift {
            bytes.extend(key_bytes("ShiftLeft", false).unwrap_or_default());
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_go_as_make_and_break_codes() {
        assert_eq!(key_bytes("KeyA", true), Some(vec![0x1C]));
        assert_eq!(key_bytes("KeyA", false), Some(vec![0xF0, 0x1C]));
        assert_eq!(key_bytes("ArrowUp", true), Some(vec![0xE0, 0x75]));
        assert_eq!(key_bytes("ArrowUp", false), Some(vec![0xE0, 0xF0, 0x75]));
        assert_eq!(key_bytes("IntlRo", true), Some(vec![0x51]));
        assert_eq!(key_bytes("MediaPlayPause", true), None);
    }

    #[test]
    fn text_is_typed_with_shift_where_needed() {
        assert_eq!(text_bytes("a"), vec![0x1C, 0xF0, 0x1C]);
        assert_eq!(
            text_bytes("A!"),
            vec![
                0x12, 0x1C, 0xF0, 0x1C, 0xF0, 0x12, // Shift+A
                0x12, 0x16, 0xF0, 0x16, 0xF0, 0x12, // Shift+1
            ]
        );
        // Caracteres sem tecla no teclado americano ficam de fora.
        assert_eq!(text_bytes("ç"), Vec::<u8>::new());
    }
}
