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
        "error",
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

#[test]
fn test_no_catalogue_key_is_unused() {
    // 16 keys sat unused; two of them were requirements nobody had wired.
    let sources = common::concat_sources("src");
    let unused: Vec<String> = keys_of("en")
        .into_iter()
        .filter(|k| !sources.contains(&format!("\"{}\"", k)))
        .collect();
    assert!(unused.is_empty(), "unused keys: {:?}", unused);
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
