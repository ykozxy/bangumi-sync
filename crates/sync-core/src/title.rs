pub(crate) fn normalize_title(value: &str) -> String {
    let mut normalized = String::new();

    for character in value.chars().map(fold_full_width_ascii) {
        if matches!(character, '\'' | '’' | 'ʼ') {
            continue;
        }
        if character == '×' {
            normalized.push(' ');
            normalized.push('x');
            normalized.push(' ');
            continue;
        }
        if let Some(folded) = fold_latin_diacritic(character) {
            normalized.push_str(folded);
            continue;
        }
        if let Some(folded) = fold_roman_numeral(character) {
            normalized.push_str(folded);
            continue;
        }
        if is_combining_mark(character) {
            continue;
        }
        if character.is_alphanumeric() {
            for lower in character.to_lowercase() {
                normalized.push(lower);
            }
        } else {
            normalized.push(' ');
        }
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn title_fts_text(value: &str) -> String {
    let normalized = normalize_title(value);
    if normalized.is_empty() || normalized == value {
        value.to_owned()
    } else {
        format!("{value} {normalized}")
    }
}

pub(crate) fn title_fts_text_for_values(values: &[String]) -> String {
    values
        .iter()
        .map(|value| title_fts_text(value))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fold_full_width_ascii(character: char) -> char {
    let codepoint = character as u32;
    if (0xFF01..=0xFF5E).contains(&codepoint) {
        char::from_u32(codepoint - 0xFEE0).expect("full-width ASCII fold should be valid")
    } else {
        character
    }
}

fn is_combining_mark(character: char) -> bool {
    matches!(
        character,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

fn fold_latin_diacritic(character: char) -> Option<&'static str> {
    match character {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' | 'Ǎ' | 'Ȁ' | 'Ȃ' | 'Ạ' | 'Ả' | 'Ấ'
        | 'Ầ' | 'Ẩ' | 'Ẫ' | 'Ậ' | 'Ắ' | 'Ằ' | 'Ẳ' | 'Ẵ' | 'Ặ' | 'à' | 'á' | 'â' | 'ã' | 'ä'
        | 'å' | 'ā' | 'ă' | 'ą' | 'ǎ' | 'ȁ' | 'ȃ' | 'ạ' | 'ả' | 'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ'
        | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => Some("a"),
        'Æ' | 'Ǽ' | 'æ' | 'ǽ' => Some("ae"),
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' | 'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => Some("c"),
        'Ð' | 'Ď' | 'Đ' | 'ð' | 'ď' | 'đ' => Some("d"),
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' | 'Ȅ' | 'Ȇ' | 'Ẹ' | 'Ẻ' | 'Ẽ' | 'Ế'
        | 'Ề' | 'Ể' | 'Ễ' | 'Ệ' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'ȅ'
        | 'ȇ' | 'ẹ' | 'ẻ' | 'ẽ' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => Some("e"),
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' | 'Ǧ' | 'ĝ' | 'ğ' | 'ġ' | 'ģ' | 'ǧ' => Some("g"),
        'Ĥ' | 'Ħ' | 'ĥ' | 'ħ' => Some("h"),
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' | 'Ǐ' | 'Ȉ' | 'Ȋ' | 'Ị' | 'Ỉ' | 'ì'
        | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'ǐ' | 'ȉ' | 'ȋ' | 'ị' | 'ỉ' => {
            Some("i")
        }
        'Ĵ' | 'ĵ' => Some("j"),
        'Ķ' | 'ķ' => Some("k"),
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' | 'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => Some("l"),
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'ñ' | 'ń' | 'ņ' | 'ň' | 'ŉ' => Some("n"),
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' | 'Ơ' | 'Ǒ' | 'Ȍ' | 'Ȏ' | 'Ọ' | 'Ỏ'
        | 'Ố' | 'Ồ' | 'Ổ' | 'Ỗ' | 'Ộ' | 'Ớ' | 'Ờ' | 'Ở' | 'Ỡ' | 'Ợ' | 'ò' | 'ó' | 'ô' | 'õ'
        | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'ơ' | 'ǒ' | 'ȍ' | 'ȏ' | 'ọ' | 'ỏ' | 'ố' | 'ồ' | 'ổ'
        | 'ỗ' | 'ộ' | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => Some("o"),
        'Œ' | 'œ' => Some("oe"),
        'Ŕ' | 'Ŗ' | 'Ř' | 'ŕ' | 'ŗ' | 'ř' => Some("r"),
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'Ș' | 'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => Some("s"),
        'ß' => Some("ss"),
        'Ţ' | 'Ť' | 'Ŧ' | 'Ț' | 'ţ' | 'ť' | 'ŧ' | 'ț' => Some("t"),
        'Þ' | 'þ' => Some("th"),
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' | 'Ư' | 'Ǔ' | 'Ȕ' | 'Ȗ' | 'Ụ'
        | 'Ủ' | 'Ứ' | 'Ừ' | 'Ử' | 'Ữ' | 'Ự' | 'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů'
        | 'ű' | 'ų' | 'ư' | 'ǔ' | 'ȕ' | 'ȗ' | 'ụ' | 'ủ' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => {
            Some("u")
        }
        'Ŵ' | 'Ẁ' | 'Ẃ' | 'Ẅ' | 'ŵ' | 'ẁ' | 'ẃ' | 'ẅ' => Some("w"),
        'Ý' | 'Ŷ' | 'Ÿ' | 'Ỳ' | 'Ỵ' | 'Ỷ' | 'Ỹ' | 'ý' | 'ÿ' | 'ŷ' | 'ỳ' | 'ỵ' | 'ỷ' | 'ỹ' => {
            Some("y")
        }
        'Ź' | 'Ż' | 'Ž' | 'ź' | 'ż' | 'ž' => Some("z"),
        _ => None,
    }
}

fn fold_roman_numeral(character: char) -> Option<&'static str> {
    match character {
        'Ⅰ' | 'ⅰ' => Some("i"),
        'Ⅱ' | 'ⅱ' => Some("ii"),
        'Ⅲ' | 'ⅲ' => Some("iii"),
        'Ⅳ' | 'ⅳ' => Some("iv"),
        'Ⅴ' | 'ⅴ' => Some("v"),
        'Ⅵ' | 'ⅵ' => Some("vi"),
        'Ⅶ' | 'ⅶ' => Some("vii"),
        'Ⅷ' | 'ⅷ' => Some("viii"),
        'Ⅸ' | 'ⅸ' => Some("ix"),
        'Ⅹ' | 'ⅹ' => Some("x"),
        'Ⅺ' | 'ⅺ' => Some("xi"),
        'Ⅻ' | 'ⅻ' => Some("xii"),
        'Ⅼ' | 'ⅼ' => Some("l"),
        'Ⅽ' | 'ⅽ' => Some("c"),
        'Ⅾ' | 'ⅾ' => Some("d"),
        'Ⅿ' | 'ⅿ' => Some("m"),
        _ => None,
    }
}
