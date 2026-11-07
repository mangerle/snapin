    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct ColorHolder {
        color: HexColor,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct HotkeyHolder {
        hotkey: Hotkey,
    }

    #[test]
    fn test_hex_color_parsing_and_serde() {
        let col = HexColor::from_hex("#00AEFF").unwrap();
        assert_eq!(col.to_rgb(), (0, 174, 255));
        assert_eq!(col.to_hex_string(), "#00AEFF");

        let col_short = HexColor::from_hex("#FFF").unwrap();
        assert_eq!(col_short.to_rgb(), (255, 255, 255));

        let holder = ColorHolder { color: col };
        let toml_str = toml::to_string(&holder).unwrap();
        assert!(toml_str.contains("\"#00AEFF\""));
        let decoded: ColorHolder = toml::from_str(&toml_str).unwrap();
        assert_eq!(decoded, holder);
    }

    /// 禁用态序列化为稳定 ASCII `none`，同时兼容旧版中文与空串输入。
    #[test]
    fn test_disabled_hotkey_uses_stable_config_encoding() {
        let disabled = Hotkey::parse_str("").expect("空绑定应解析为禁用态");
        assert!(disabled.is_disabled());
        assert_eq!(disabled.to_display_string(), "已禁用");

        let holder = HotkeyHolder {
            hotkey: disabled.clone(),
        };
        let toml_str = toml::to_string(&holder).expect("禁用态应可序列化");
        assert!(
            toml_str.contains("none"),
            "配置应写入稳定 ASCII none: {toml_str}"
        );
        let decoded: HotkeyHolder = toml::from_str(&toml_str).expect("禁用态应可反序列化");
        assert!(decoded.hotkey.is_disabled());

        for legacy in ["已禁用", "none", "  "] {
            let parsed = Hotkey::parse_str(legacy).expect("旧版禁用文本应可解析");
            assert!(parsed.is_disabled(), "{legacy} 应解析为禁用态");
        }
    }

    #[test]
    fn test_hex_color_non_ascii_is_rejected_without_panic() {
        for input in ["#红色", "#测试", "#ééé", "#１２３", "红色", "１２３"] {
            assert_eq!(
                HexColor::from_hex(input),
                None,
                "非 ASCII 输入必须安全返回 None: {input}"
            );
        }

        let rgba = HexColor::from_hex("#00AEFF80").expect("8 位颜色应解析成功");
        assert_eq!(rgba.to_rgba(), (0, 174, 255, 128));
    }

    #[test]
    fn test_hotkey_parsing_and_serde() {
        let hk = Hotkey::parse_str("Ctrl+Alt+A").expect("合法组合键应解析成功");
        assert!(hk.ctrl() && hk.alt() && !hk.shift());
        assert_eq!(hk.key(), "A");
        assert_eq!(hk.to_display_string(), "Ctrl+Alt+A");

        let f1 = Hotkey::parse_str("F1").expect("单功能键应解析成功");
        assert_eq!(f1.to_display_string(), "F1");

        let holder = HotkeyHolder { hotkey: hk };
        let toml_str = toml::to_string(&holder).unwrap();
        let decoded: HotkeyHolder = toml::from_str(&toml_str).unwrap();
        assert_eq!(decoded, holder);
    }

    /// 录制器输出的全部合法词表必须通过校验，且展示名可稳定往返
    #[test]
    fn test_hotkey_accepts_recorder_vocabulary() {
        let accepted = [
            "F1",
            "f12",
            "F24",
            "A",
            "7",
            "Space",
            "space",
            "Enter",
            "Tab",
            "Print",
            "Pause",
            "Insert",
            "Delete",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Up",
            "Down",
            "Left",
            "Right",
            ",",
            ".",
            ";",
            "Ctrl+F3",
            "Shift+F3",
            "Ctrl+Shift+S",
            "Super+A",
            "Win+D",
            "Meta+F10",
            "Alt+Space",
            "已禁用",
            "none",
            "",
        ];
        for binding in accepted {
            let hotkey = Hotkey::parse_str(binding)
                .unwrap_or_else(|e| panic!("快捷键 {binding} 应通过校验，实际失败: {e}"));
            let display = hotkey.to_display_string();
            let round_trip = Hotkey::parse_str(&display)
                .unwrap_or_else(|e| panic!("快捷键展示名 {display} 应可往返解析，实际失败: {e}"));
            assert_eq!(round_trip, hotkey, "快捷键 {binding} 往返不一致");
        }
    }

    /// 拼错的修饰键、缺失主键与超范围功能键必须显式报错，杜绝静默注册无效热键
    #[test]
    fn test_hotkey_rejects_invalid_bindings() {
        assert_eq!(
            Hotkey::parse_str("Crtl+A"),
            Err(HotkeyParseError::UnknownModifier {
                binding: "Crtl+A".to_string(),
                token: "Crtl".to_string(),
            })
        );
        assert_eq!(
            Hotkey::parse_str("Ctrl+"),
            Err(HotkeyParseError::MissingMainKey {
                binding: "Ctrl+".to_string(),
            })
        );
        assert_eq!(
            Hotkey::parse_str("Ctrl+F25"),
            Err(HotkeyParseError::UnsupportedMainKey {
                binding: "Ctrl+F25".to_string(),
                key: "F25".to_string(),
            })
        );
        assert_eq!(
            Hotkey::parse_str("F0"),
            Err(HotkeyParseError::UnsupportedMainKey {
                binding: "F0".to_string(),
                key: "F0".to_string(),
            })
        );
        assert_eq!(
            Hotkey::parse_str("Ctrl+日本語"),
            Err(HotkeyParseError::UnsupportedMainKey {
                binding: "Ctrl+日本語".to_string(),
                key: "日本語".to_string(),
            })
        );
        assert!(matches!(
            Hotkey::parse_str("<Control>F3"),
            Err(HotkeyParseError::UnsupportedMainKey { .. })
        ));
    }

    /// 配置反序列化必须把非法快捷键转成带中文原因的 serde 错误，而不是静默接受无效键
    #[test]
    fn test_hotkey_deserialize_rejects_invalid_binding() {
        let error = toml::from_str::<HotkeyHolder>("hotkey = \"Hyper+B\"")
            .expect_err("非法修饰键应导致配置解析失败");
        assert!(error.message().contains("Hyper"));
    }
