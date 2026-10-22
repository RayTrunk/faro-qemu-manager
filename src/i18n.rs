use std::collections::HashMap;

/// Embedded default catalogs bundled into the binary
const EN_DEFAULT: &str = include_str!("../assets/locales/en.json");
const DE_DEFAULT: &str = include_str!("../assets/locales/de.json");
const FR_DEFAULT: &str = include_str!("../assets/locales/fr.json");

/// Static list of available locales as code and display name pairs
pub const LOCALES: &[(&str, &str)] = &[("en", "English"), ("de", "Deutsch"), ("fr", "Français")];

/// Multilingual internationalization manager supporting EN, DE, FR
#[derive(Debug, Clone)]
pub struct I18n {
    current_locale: String,
    catalogs: HashMap<String, HashMap<String, String>>,
}

impl Default for I18n {
    fn default() -> Self {
        Self::new("en")
    }
}

impl I18n {
    /// Creates a new `I18n` instance initialized with the given locale code.
    pub fn new(locale: &str) -> Self {
        let mut catalogs = HashMap::new();

        if let Ok(en_map) = serde_json::from_str::<HashMap<String, String>>(EN_DEFAULT) {
            catalogs.insert("en".to_string(), en_map);
        }
        if let Ok(de_map) = serde_json::from_str::<HashMap<String, String>>(DE_DEFAULT) {
            catalogs.insert("de".to_string(), de_map);
        }
        if let Ok(fr_map) = serde_json::from_str::<HashMap<String, String>>(FR_DEFAULT) {
            catalogs.insert("fr".to_string(), fr_map);
        }

        Self {
            current_locale: locale.to_string(),
            catalogs,
        }
    }

    /// List of available locales as static code and display name pairs
    pub fn available_locales(&self) -> Vec<(&'static str, &'static str)> {
        LOCALES.to_vec()
    }

    /// Static accessor for available locales
    pub fn locales() -> &'static [(&'static str, &'static str)] {
        LOCALES
    }

    /// Returns the currently active locale code
    pub fn current_locale(&self) -> &str {
        &self.current_locale
    }

    /// Sets the active locale code
    pub fn set_locale(&mut self, locale: &str) {
        self.current_locale = locale.to_string();
    }

    /// Translates a key according to the active locale.
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(cat) = self.catalogs.get(&self.current_locale) {
            if let Some(val) = cat.get(key) {
                return val.as_str();
            }
        }
        if self.current_locale != "en" {
            if let Some(en_cat) = self.catalogs.get("en") {
                if let Some(val) = en_cat.get(key) {
                    return val.as_str();
                }
            }
        }
        key
    }

    /// Helper for string formatting with placeholder replacement: `{key}` -> `value`
    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut template = self.t(key).to_string();
        for (param, val) in args {
            template = template.replace(&format!("{{{}}}", param), val);
        }
        template
    }
}
