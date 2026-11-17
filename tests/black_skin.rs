use qingyu::model::{Settings,Skin};

#[test]
fn legacy_material_settings_do_not_override_current_preferences() {
    let path = std::env::temp_dir().join(format!("qingyu-black-skin-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    std::env::set_var("QINGYU_DATA_DIR", &path);
    for skin in ["glass", "clear", "black"] {
        std::fs::write(path.join("settings.txt"), format!("material={skin}\npet=0\nclock_12h=1\n")).unwrap();
        let mut settings = Settings::load();
        // The resident companion uses its own preference; the retired pet
        // module must not hide it when importing an old settings file.
        assert!(settings.pet);
        assert!(settings.clock_12h);
        assert_eq!(settings.skin,Settings::default().skin);
        settings.skin=Skin::Cream;
        settings.pet=false;
        settings.save();
        // Unknown legacy fields may remain for data compatibility; only the
        // current preferences can affect the restored app after another load.
        let restored=Settings::load();
        assert_eq!(restored.skin,Skin::Cream);
        assert!(!restored.pet);
        assert!(restored.clock_12h);
    }
    std::fs::remove_file(path.join("settings.txt")).unwrap();
    std::fs::remove_dir(path).unwrap();
}
