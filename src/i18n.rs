use std::collections::HashMap;
use std::path::Path;

/// Embedded default catalogs bundled into the binary
const EN_DEFAULT: &str = include_str!("../assets/locales/en.json");
const DE_DEFAULT: &str = include_str!("../assets/locales/de.json");
const FR_DEFAULT: &str = include_str!("../assets/locales/fr.json");

/// Multilingual internationalization manager supporting EN, DE, FR
/// with runtime fallbacks and custom JSON override catalogs.
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
    /// Default catalogs (en, de, fr) are baked into the binary.
    /// If an external `locales/` directory is present, overrides are loaded automatically.
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

        let mut i18n = Self {
            current_locale: locale.to_string(),
            catalogs,
        };

        // Scan for external override directory if present
        if Path::new("locales").is_dir() {
            i18n.load_overrides_from_dir("locales");
        }

        i18n
    }

    /// List of available locales as static code and display name pairs: `[("en", "English"), ("de", "Deutsch"), ("fr", "Français")]`
    pub fn locales() -> Vec<(&'static str, &'static str)> {
        vec![("en", "English"), ("de", "Deutsch"), ("fr", "Français")]
    }

    /// Available locales accessor matching interface requirements
    pub fn available_locales(&self) -> Vec<(&str, &str)> {
        vec![("en", "English"), ("de", "Deutsch"), ("fr", "Français")]
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
    /// Fallback chain:
    /// 1. Current locale catalog
    /// 2. English ("en") catalog (if current locale is not English)
    /// 3. The raw key string itself
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

    /// Loads or merges an override catalog from a JSON string for a given locale
    pub fn load_override_json(
        &mut self,
        locale: &str,
        json_content: &str,
    ) -> Result<(), serde_json::Error> {
        let map: HashMap<String, String> = serde_json::from_str(json_content)?;
        self.catalogs
            .entry(locale.to_string())
            .or_default()
            .extend(map);
        Ok(())
    }

    /// Loads or merges an override catalog from a file for a given locale
    pub fn load_override_file<P: AsRef<Path>>(
        &mut self,
        locale: &str,
        path: P,
    ) -> std::io::Result<()> {
        let content = std::fs::read_to_string(path)?;
        if let Err(e) = self.load_override_json(locale, &content) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e));
        }
        Ok(())
    }

    /// Scans a directory for `{locale}.json` files and merges their entries
    pub fn load_overrides_from_dir<P: AsRef<Path>>(&mut self, dir: P) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let _ = self.load_override_file(stem, &path);
                    }
                }
            }
        }
    }
}
