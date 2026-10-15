pub fn resolve(value: &str) -> &'static str {
    let normalized = value.replace('_', "-").to_ascii_lowercase();
    if normalized == "zh-yue" || normalized.starts_with("zh-yue-") {
        return "yue-HK";
    }
    match normalized.split(['-', '.', '@']).next().unwrap_or("") {
        "zh" => "zh-CN",
        "hi" => "hi-IN",
        "es" => "es-ES",
        "ar" => "ar-SA",
        "fr" => "fr-FR",
        "bn" => "bn-BD",
        "pt" => "pt-BR",
        "ru" => "ru-RU",
        "ur" => "ur-PK",
        "id" => "id-ID",
        "de" => "de-DE",
        "ja" => "ja-JP",
        "pcm" => "pcm-NG",
        "mr" => "mr-IN",
        "te" => "te-IN",
        "tr" => "tr-TR",
        "ta" => "ta-IN",
        "yue" => "yue-HK",
        "vi" => "vi-VN",
        _ => "en-US",
    }
}

pub fn system() -> &'static str {
    #[cfg(windows)]
    {
        let mut buffer = [0u16; 85];
        let length = unsafe {
            windows_sys::Win32::Globalization::GetUserDefaultLocaleName(
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            )
        };
        if length > 1 {
            return resolve(&String::from_utf16_lossy(&buffer[..length as usize - 1]));
        }
    }
    #[cfg(not(windows))]
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                return resolve(&value);
            }
        }
    }
    "en-US"
}

#[cfg(test)]
mod tests {
    #[test]
    fn regional_script_and_unsupported_locales_resolve() {
        for (input, expected) in [
            ("tr-TR", "tr-TR"),
            ("tr_CY.UTF-8", "tr-TR"),
            ("pt-PT", "pt-BR"),
            ("zh-Hant-TW", "zh-CN"),
            ("zh-yue-HK", "yue-HK"),
            ("yue-Hant", "yue-HK"),
            ("ko-KR", "en-US"),
            ("", "en-US"),
        ] {
            assert_eq!(super::resolve(input), expected);
        }
    }
}
