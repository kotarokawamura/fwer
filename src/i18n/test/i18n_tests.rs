use super::*;

#[test]
fn test_po_parser_basic_and_multiline() {
    let po_source = r#"
msgid ""
msgstr ""
"Language: ja\n"
"Content-Type: text/plain; charset=UTF-8\n"

msgid "Copy"
msgstr "コピー"

msgid "Multi"
"Line"
msgstr "複数"
"行\nテスト"
"#;
    let cat = PoCatalog::parse(po_source, "en");
    assert_eq!(cat.locale, "ja");
    assert_eq!(cat.get("Copy"), "コピー");
    assert_eq!(cat.get("MultiLine"), "複数行\nテスト");
    assert_eq!(cat.get("Unknown"), "Unknown");
}

#[test]
fn test_po_parser_fuzzy_is_skipped() {
    let po_source = r#"
#, fuzzy
msgid "Open Settings"
msgstr "仮の設定"

msgid "Close Pane"
msgstr "ペインを閉じる"
"#;
    let cat = PoCatalog::parse(po_source, "ja");
    assert_eq!(cat.get("Open Settings"), "Open Settings");
    assert_eq!(cat.get("Close Pane"), "ペインを閉じる");
}

#[test]
fn test_po_parser_context_and_plurals() {
    let po_source = r#"
msgctxt "menu"
msgid "Open"
msgstr "メニューを開く"

msgid "%d pane"
msgid_plural "%d panes"
msgstr[0] "%d 個のペイン (単数)"
msgstr[1] "%d 個のペイン (複数)"
"#;
    let cat = PoCatalog::parse(po_source, "ja");
    assert_eq!(cat.get_ctx("menu", "Open"), "メニューを開く");
    assert_eq!(cat.get_ctx("other", "Open"), "Open");
    assert_eq!(cat.get_plural("%d pane", "%d panes", 1), "%d 個のペイン (単数)");
    assert_eq!(cat.get_plural("%d pane", "%d panes", 3), "%d 個のペイン (複数)");
}

#[test]
fn test_extract_lang_code() {
    assert_eq!(extract_lang_code("ja_JP.UTF-8"), Some("ja".to_string()));
    assert_eq!(extract_lang_code("en_US.utf8"), Some("en".to_string()));
    assert_eq!(extract_lang_code("C"), None);
    assert_eq!(extract_lang_code("POSIX"), None);
    assert_eq!(extract_lang_code("C.UTF-8"), None);
}
