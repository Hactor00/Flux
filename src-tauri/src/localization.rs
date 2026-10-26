pub fn supported(locale: &str) -> bool {
    matches!(
        locale,
        "en-US"
            | "zh-CN"
            | "hi-IN"
            | "es-ES"
            | "ar-SA"
            | "fr-FR"
            | "bn-BD"
            | "pt-BR"
            | "ru-RU"
            | "ur-PK"
            | "id-ID"
            | "de-DE"
            | "ja-JP"
            | "pcm-NG"
            | "mr-IN"
            | "te-IN"
            | "tr-TR"
            | "ta-IN"
            | "yue-HK"
            | "vi-VN"
    )
}

pub fn words(locale: &str) -> serde_json::Value {
    let source = match locale {
        "en-US" => include_str!("../../src/locales/en-US.json"),
        "zh-CN" => include_str!("../../src/locales/zh-CN.json"),
        "hi-IN" => include_str!("../../src/locales/hi-IN.json"),
        "es-ES" => include_str!("../../src/locales/es-ES.json"),
        "ar-SA" => include_str!("../../src/locales/ar-SA.json"),
        "fr-FR" => include_str!("../../src/locales/fr-FR.json"),
        "bn-BD" => include_str!("../../src/locales/bn-BD.json"),
        "pt-BR" => include_str!("../../src/locales/pt-BR.json"),
        "ru-RU" => include_str!("../../src/locales/ru-RU.json"),
        "ur-PK" => include_str!("../../src/locales/ur-PK.json"),
        "id-ID" => include_str!("../../src/locales/id-ID.json"),
        "de-DE" => include_str!("../../src/locales/de-DE.json"),
        "ja-JP" => include_str!("../../src/locales/ja-JP.json"),
        "pcm-NG" => include_str!("../../src/locales/pcm-NG.json"),
        "mr-IN" => include_str!("../../src/locales/mr-IN.json"),
        "te-IN" => include_str!("../../src/locales/te-IN.json"),
        "tr-TR" => include_str!("../../src/locales/tr-TR.json"),
        "ta-IN" => include_str!("../../src/locales/ta-IN.json"),
        "yue-HK" => include_str!("../../src/locales/yue-HK.json"),
        "vi-VN" => include_str!("../../src/locales/vi-VN.json"),
        _ => include_str!("../../src/locales/en-US.json"),
    };
    serde_json::from_str(source).expect("Validated localization catalog")
}
