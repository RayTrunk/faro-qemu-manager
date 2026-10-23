mod common;

use faro_qem::i18n::I18n;

/// Keys present in one catalogue, read from the file itself.
fn keys_of(lang: &str) -> std::collections::BTreeSet<String> {
    common::catalogue_keys(lang)
}

#[test]
fn test_i18n_locales_and_translations() {
    let mut i18n = I18n::new("en");
    assert_eq!(i18n.t("new_vm"), "New VM");
    assert_eq!(i18n.t("launch"), "Launch VM");

    i18n.set_locale("de");
    assert_eq!(i18n.t("new_vm"), "Neue VM");
    assert_eq!(i18n.t("launch"), "VM Starten");

    i18n.set_locale("fr");
    assert_eq!(i18n.t("new_vm"), "Nouvelle MV");
    assert_eq!(i18n.t("launch"), "Lancer MV");
}

#[test]
fn test_i18n_fallback_to_key_or_en() {
    let i18n = I18n::new("de");
    // If key does not exist in any catalog, return key
    assert_eq!(i18n.t("non_existent_key"), "non_existent_key");
}

#[test]
fn test_i18n_available_locales() {
    let i18n = I18n::new("en");
    let locales = i18n.available_locales();
    assert_eq!(locales.len(), 3);
    assert!(locales.contains(&("en", "English")));
    assert!(locales.contains(&("de", "Deutsch")));
    assert!(locales.contains(&("fr", "Français")));
}

#[test]
fn test_i18n_current_locale_and_unknown_default() {
    let mut i18n = I18n::new("en");
    assert_eq!(i18n.current_locale(), "en");

    i18n.set_locale("de");
    assert_eq!(i18n.current_locale(), "de");

    i18n.set_locale("fr");
    assert_eq!(i18n.current_locale(), "fr");

    // Setting an unknown locale
    i18n.set_locale("es");
    assert_eq!(i18n.current_locale(), "es");
    // Fallback should still return English or key
    assert_eq!(i18n.t("new_vm"), "New VM");
}

#[test]
fn test_i18n_rich_catalog_keys_present_in_all_locales() {
    let required_keys = [
        // App header & brand
        "app_title",
        "app_copyright",
        // Actions
        "launch",
        "stop",
        "save",
        "delete",
        "duplicate",
        "new_vm",
        "add_disk",
        "browse",
        "eject",
        "clear",
        "copy",
        // Tabs
        "tab_system",
        "tab_storage",
        "tab_display",
        "tab_network",
        "tab_console",
        // Hardware fields
        "arch",
        "machine",
        "cpu",
        "cores",
        "memory_mb",
        "accel",
        "firmware",
        // Storage fields
        "format",
        "interface",
        "boot_index",
        "size_gb",
        "create_disk",
        "iso_path",
        // Display & Network fields
        "vga_type",
        "backend",
        "fullscreen",
        "nic_model",
        "host_port",
        "guest_port",
        "extra_args",
        // Status messages
        "status_running",
        "status_stopped",
        "status_error",
        // Settings modal
        "language",
        "theme",
        "dark",
        "light",
        "qemu_dir",
        "close",
        "add_cdrom",
        "action",
        "delete_confirm_msg",
        "auto_scroll",
        "copy_logs",
        "lines_label",
        "filter_placeholder",
        // Menu bar items
        "menu_file",
        "menu_vm",
        "menu_help",
        "about",
        "exit",
    ];

    for locale in ["en", "de", "fr"] {
        let i18n = I18n::new(locale);
        for key in &required_keys {
            let translated = i18n.t(key);
            assert_ne!(
                translated, *key,
                "Locale '{}' is missing translation for key '{}'",
                locale, key
            );
            assert!(
                !translated.trim().is_empty(),
                "Translation for key '{}' in locale '{}' is empty",
                key,
                locale
            );
        }
    }
}

#[test]
fn test_i18n_template_formatting_and_default() {
    let i18n = I18n::default();
    assert_eq!(i18n.current_locale(), "en");

    let formatted = i18n.tf("non_existent_{vm}", &[("vm", "Debian-12")]);
    assert_eq!(formatted, "non_existent_Debian-12");
}

// --------------------------------------------------------------------------
// Part 3a, task 8: the interface speaks the chosen language everywhere
// --------------------------------------------------------------------------

#[test]
fn test_the_three_catalogues_hold_the_same_keys() {
    // A key only English has reads as English mid-sentence for everyone else,
    // because `I18n::t` falls back silently.
    let en = keys_of("en");
    for lang in ["de", "fr"] {
        let theirs = keys_of(lang);
        let missing: Vec<&String> = en.difference(&theirs).collect();
        let extra: Vec<&String> = theirs.difference(&en).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{} differs from en — missing: {:?}, extra: {:?}",
            lang,
            missing,
            extra
        );
    }
}

/// Every key the sources actually ask the catalogue for.
///
/// Taken from `.t("…")` and `.tf("…")` call sites rather than by searching for
/// the key's name anywhere in the text: a bare substring search counts a key
/// as used when its name happens to appear as a `tf` placeholder, a serde
/// rename or a QEMU option, which is how a dead key once looked alive.
fn keys_asked_for() -> std::collections::BTreeSet<String> {
    let sources = common::concat_sources("src");
    let mut keys = std::collections::BTreeSet::new();

    // A key is the first string literal after one of these openings. A
    // placeholder name, by contrast, is the first literal after a bare `(`,
    // which is how `("error", …)` used to pass for a use of the key `error`.
    for opening in [".t(", ".tf(", "i18n,"] {
        let mut rest = sources.as_str();
        while let Some(at) = rest.find(opening) {
            rest = &rest[at + opening.len()..];
            let after = rest.trim_start();
            if let Some(literal) = after.strip_prefix('"') {
                if let Some(end) = literal.find('"') {
                    keys.insert(literal[..end].to_string());
                }
            }
        }
    }

    keys
}

/// Every name that appears as a `{placeholder}` in some catalogue text.
///
/// These are the reason a bare search cannot answer "is this key used": a key
/// named like a placeholder looks alive because `("error", …)` sits in the
/// sources. Knowing the set lets the unused-key check discount exactly that.
fn placeholder_names() -> std::collections::BTreeSet<String> {
    let text = std::fs::read_to_string("assets/locales/en.json").expect("read en.json");
    let mut names = std::collections::BTreeSet::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find('{') {
        rest = &rest[at + 1..];
        if let Some(end) = rest.find('}') {
            let name = &rest[..end];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase()) {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// Whether the sources mention this key at all, as a whole literal.
///
/// Used only for keys the code reaches through a variable — the templates'
/// notices, the wizard's reasons, the duplicate's shared-image reasons — and
/// never for a name that is also a placeholder.
fn mentioned_as_literal(sources: &str, key: &str) -> bool {
    !placeholder_names().contains(key) && sources.contains(&format!("\"{}\"", key))
}

#[test]
fn test_no_catalogue_key_is_unused() {
    // 16 keys sat unused; two of them were requirements nobody had wired.
    let asked = keys_asked_for();
    let sources = common::concat_sources("src");
    let unused: Vec<String> = keys_of("en")
        .into_iter()
        .filter(|k| !asked.contains(k) && !mentioned_as_literal(&sources, k))
        .collect();
    assert!(unused.is_empty(), "unused keys: {:?}", unused);
}

#[test]
fn test_every_key_the_code_asks_for_exists_in_every_catalogue() {
    // The direction nothing guarded: a key the catalogue lacks is not an
    // error, it renders its own name on screen. `I18n::t` falls back to
    // English and then to the key itself, so the suite must be the thing
    // that notices.
    for lang in ["en", "de", "fr"] {
        let have = keys_of(lang);
        let missing: Vec<&String> = keys_asked_for()
            .iter()
            .filter(|k| !have.contains(*k))
            .filter(|k| !k.contains('{'))
            .cloned()
            .collect::<Vec<String>>()
            .leak()
            .iter()
            .collect();
        assert!(
            missing.is_empty(),
            "{} has no entry for: {:?} — these render as their own names",
            lang,
            missing
        );
    }
}

#[test]
fn test_the_interface_holds_no_hardcoded_english_hint() {
    // German and French users read English mid-screen today.
    let sources = common::concat_sources("src");
    for needle in [
        "(SLIRP NAT user-mode network with Internet access)",
        "(Host bridged interface)",
        "(No network connectivity)",
        "(Auto: WHPX on Windows)",
    ] {
        assert!(!sources.contains(needle), "still hardcoded: {}", needle);
    }
}

#[test]
fn test_nothing_in_the_ui_is_dead() {
    // The plan names SettingsDialog::with_logo, which had no caller. The
    // sidebar's and the detail panel's namesakes do have one (app.rs), so the
    // check is on this file rather than the whole tree.
    let settings = std::fs::read_to_string("src/ui/settings.rs").expect("read settings.rs");
    assert!(
        !settings.contains("fn with_logo"),
        "SettingsDialog::with_logo has no caller"
    );
}
