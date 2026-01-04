use crate::settings::Settings;

/// Supported languages: (code, display name)
pub const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("ko", "한국어"),
    ("ja", "日本語"),
];

/// Initialize language from settings or system locale
pub fn init_language(settings: &Settings) {
    let locale = if let Some(ref lang) = settings.language {
        // User preference
        lang.clone()
    } else {
        // Auto-detect from system
        detect_system_language()
    };

    rust_i18n::set_locale(&locale);
    log::info!("Language set to: {}", locale);
}

/// Detect system language and map to supported language
fn detect_system_language() -> String {
    if let Some(locale) = sys_locale::get_locale() {
        log::info!("System locale detected: {}", locale);

        // locale might be "en-US", "ko-KR", "ja-JP", etc.
        // Extract the language code (first part before '-')
        let lang = locale.split('-').next().unwrap_or("en");

        // Check if supported
        if SUPPORTED_LANGUAGES
            .iter()
            .any(|(code, _)| *code == lang)
        {
            return lang.to_string();
        }
    }

    // Fallback to English
    log::info!("Falling back to English");
    "en".to_string()
}

/// Get current language display name
#[allow(dead_code)]
pub fn current_language_name() -> String {
    let current = rust_i18n::locale().to_string();
    SUPPORTED_LANGUAGES
        .iter()
        .find(|(code, _)| *code == current.as_str())
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| "English".to_string())
}

/// Get current language code
pub fn current_language() -> String {
    rust_i18n::locale().to_string()
}

/// Change the current language and save to settings
pub fn change_language(lang: &str) {
    if SUPPORTED_LANGUAGES.iter().any(|(code, _)| *code == lang) {
        rust_i18n::set_locale(lang);
        log::info!("Language changed to: {}", lang);
    } else {
        log::warn!("Attempted to set unsupported language: {}", lang);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_languages() {
        assert_eq!(SUPPORTED_LANGUAGES.len(), 3);
        assert!(SUPPORTED_LANGUAGES.iter().any(|(code, _)| *code == "en"));
        assert!(SUPPORTED_LANGUAGES.iter().any(|(code, _)| *code == "ko"));
        assert!(SUPPORTED_LANGUAGES.iter().any(|(code, _)| *code == "ja"));
    }

    #[test]
    fn test_current_language_name() {
        // Default should be English
        let name = current_language_name();
        assert!(!name.is_empty());
    }
}
