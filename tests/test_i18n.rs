use faro_qem::i18n::I18n;

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
        "uefi",
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
        "running",
        "stopped",
        "error",
        // Settings modal
        "language",
        "theme",
        "dark",
        "light",
        "qemu_dir",
        "select_folder",
        "close",
        "add_cdrom",
        "action",
        "delete_confirm_msg",
        "auto_scroll",
        "copy_logs",
        "lines_label",
        "filter_placeholder",
        "filter_label",
        // Menu bar items
        "menu_file",
        "menu_vm",
        "menu_settings",
        "menu_help",
        "open_settings",
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
