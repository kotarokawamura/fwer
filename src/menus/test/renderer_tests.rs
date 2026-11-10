use super::*;
use crate::config::theme::ShortcutSettings;
use crate::menus::types::MenuAction;
use crate::ui::state::TileId;

#[test]
fn test_default_pane_and_titlebar_menus_and_plugins() {
    let mut mgr = MenuManager::new();
    let ctx_pane = MenuContext {
        target_kind: MenuTargetKind::Pane,
        tile_id: TileId(1),
        has_selection: false,
        show_scrollbar: true,
        is_read_only: false,
        shortcuts: ShortcutSettings::default(),
    };

    let pane_entries = mgr.build_entries(&ctx_pane);
    assert_eq!(pane_entries.len(), 11);

    mgr.register_plugin_entry(
        MenuTargetKind::Pane,
        MenuEntry::Item {
            icon: "🔌".to_string(),
            label: "Run Custom Plugin".to_string(),
            shortcut: None,
            action: MenuAction::PluginCommand("my_plugin".to_string()),
            enabled: true,
        },
    );
    let pane_with_plugin = mgr.build_entries(&ctx_pane);
    assert_eq!(pane_with_plugin.len(), 13); // + Separator + Plugin Item

    let ctx_titlebar = MenuContext {
        target_kind: MenuTargetKind::Titlebar,
        tile_id: TileId(1),
        has_selection: false,
        show_scrollbar: false,
        is_read_only: true,
        shortcuts: ShortcutSettings::default(),
    };
    let tb_entries = mgr.build_entries(&ctx_titlebar);
    assert_eq!(tb_entries.len(), 10);
    assert!(pane_entries.iter().any(|e| matches!(
        e,
        MenuEntry::Item {
            icon,
            action: MenuAction::ToggleScrollbar,
            ..
        } if icon == "☑"
    )));
    assert!(pane_entries.iter().any(|e| matches!(
        e,
        MenuEntry::Item {
            icon,
            action: MenuAction::ToggleReadOnly,
            ..
        } if icon == "□"
    )));
    assert!(tb_entries.iter().any(|e| matches!(
        e,
        MenuEntry::Item {
            icon,
            action: MenuAction::ToggleScrollbar,
            ..
        } if icon == "□"
    )));
    assert!(tb_entries.iter().any(|e| matches!(
        e,
        MenuEntry::Item {
            icon,
            action: MenuAction::ToggleReadOnly,
            ..
        } if icon == "☑"
    )));

    let preset_names = vec!["Dev Set".to_string(), "Logs".to_string()];
    let pane_with_presets = mgr.build_entries_with_presets(&ctx_pane, &preset_names);
    assert!(pane_with_presets.iter().any(|e| matches!(
        e,
        MenuEntry::SubMenu { entries, .. }
            if entries.len() == 2
                && matches!(&entries[0], MenuEntry::Item { action: MenuAction::ApplyPreset(name), .. } if name == "Dev Set")
    )));

    let mut disabled_shortcuts = ShortcutSettings::default();
    disabled_shortcuts.copy = String::new();
    disabled_shortcuts.paste = "Disabled".to_string();
    let ctx_disabled = MenuContext {
        target_kind: MenuTargetKind::Pane,
        tile_id: TileId(1),
        has_selection: false,
        show_scrollbar: true,
        is_read_only: false,
        shortcuts: disabled_shortcuts,
    };
    let disabled_entries = mgr.build_entries(&ctx_disabled);
    assert!(disabled_entries.iter().any(|e| matches!(
        e,
        MenuEntry::Item {
            action: MenuAction::Copy,
            shortcut: None,
            ..
        }
    )));
    assert!(disabled_entries.iter().any(|e| matches!(
        e,
        MenuEntry::Item {
            action: MenuAction::Paste,
            shortcut: None,
            ..
        }
    )));
}
