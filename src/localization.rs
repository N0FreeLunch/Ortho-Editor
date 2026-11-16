//! Shared localization lookup and template formatting.

use crate::data::localization::LOCALIZATIONS;

pub fn format_template(template: &str, args: &[String]) -> String {
    let mut out = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    let mut arg_idx = 0;

    while i < chars.len() {
        if chars[i] == '%' {
            if i + 1 < chars.len() {
                match chars[i + 1] {
                    '%' => {
                        out.push('%');
                        i += 2;
                        continue;
                    }
                    'd' | 'i' | 'u' | 's' => {
                        if let Some(val) = args.get(arg_idx) {
                            out.push_str(val);
                            arg_idx += 1;
                        }
                        i += 2;
                        continue;
                    }
                    '+' => {
                        if i + 2 < chars.len() {
                            match chars[i + 2] {
                                'd' | 'i' | 'u' => {
                                    if let Some(val) = args.get(arg_idx) {
                                        if !val.starts_with('-') && !val.starts_with('+') {
                                            out.push('+');
                                        }
                                        out.push_str(val);
                                        arg_idx += 1;
                                    }
                                    i += 3;
                                    continue;
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

pub fn get_loc(key: &str, language: &str) -> &'static str {
    get_loc_optional(key, language).unwrap_or_else(|| Box::leak(key.to_string().into_boxed_str()))
}

pub fn get_loc_optional(key: &str, language: &str) -> Option<&'static str> {
    LOCALIZATIONS
        .iter()
        .find(|l| l.key == key)
        .map(|l| if language == "ko" { l.ko } else { l.en })
}

pub fn strip_d2_color_codes(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{00FF}' && i + 1 < chars.len() && chars[i + 1] == 'c' {
            i += 3; // Skip `ÿ`, c, and the color code character
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
