# fwer 仕様書 (Specification)

対象バージョン: `0.0.3` (Rust Edition 2024)  
ライセンス: GNU General Public License v2.0 (GPL-2.0)

---

## 1. 概要

**termin** は、Rust および `eframe` / `egui` / `egui_tiles` を用いて実装された、Terminator 風のタイル型（画面分割・タブ対応）ターミナルエミュレータである。  
フレームレスウィンドウ上に各ペイン固有のカスタムタイトルバーを備え、ペインの水平・垂直分割、タブ化、ペイン位置の入れ替え、複数ペインへの同時キー入力（ブロードキャスト）、リサイズ時のテキスト自動リフロー、テーマ・フォント・ショートカット・言語（GNU gettext `.po`）のリアルタイム設定変更機能を提供する。

### 1.1 主要依存クレート

| クレート | バージョン | 用途 |
| :--- | :--- | :--- |
| `eframe` / `egui` | `0.37.0` | GUI フレームワーク・2D レンダリング・イベント処理 |
| `egui_tiles` | `0.18.0` | 分割ペインおよびタブのツリーレイアウト管理 |
| `portable-pty` | `0.9.0` | 擬似端末 (PTY) の生成・シェルプロセス管理・リサイズ |
| `vt100` | `0.18.1` | VT100 / ANSI エスケープシーケンスのパースおよび画面バッファ管理 |
| `x11rb` | `0.13.2` (`xinput`) | X11 / XWayland 環境での Super キー状態取得、XInput 2.4 ピンチズーム、起動時フォーカス制御 |
| `raw-window-handle` | `0.6.2` | ネイティブ X11 ウィンドウハンドルの取得 |
| `serde` / `serde_json` | `1.0` | 設定ファイル (`config.json`) およびレイアウト (`layout.json`) のシリアライズ・デシリアライズ |

---

## 2. ディレクトリ・モジュール構成

```text
termin/
├── Cargo.toml
├── LICENSE
├── po/
│   ├── termin.pot                # 翻訳テンプレート
│   ├── en.po                     # 英語カタログ（バイナリに埋め込み）
│   └── ja.po                     # 日本語カタログ（バイナリに埋め込み）
└── src/
    ├── main.rs                   # エントリポイント、CLI引数パース、デバッグサーバー、Viewport初期化
    ├── config/
    │   ├── mod.rs
    │   ├── theme.rs              # ThemeSettings / ShortcutSettings、カラー・ショートカット解析、システムフォント探索
    │   ├── layout_preset.rs      # LayoutNode による分割・タブ構成の保存・復元
    │   └── prefs_ui.rs           # 環境設定ウィンドウ (PreferencesWindow) とフォントサンプルカタログ画面
    ├── i18n/
    │   └── mod.rs                # GNU gettext (.po) パーサーとグローバル翻訳カタログ管理
    ├── menus/
    │   ├── mod.rs                # MenuManager（コンテキストメニュー管理・プラグイン拡張API）
    │   ├── types.rs              # MenuTargetKind, MenuContext, MenuAction, MenuEntry
    │   ├── presets.rs            # Pane / Titlebar 向けデフォルトメニュー定義
    │   └── renderer.rs           # 再帰的コンテキストメニュー描画
    ├── session/
    │   ├── mod.rs                # SessionDriver トレイト定義
    │   ├── local_pty.rs          # LocalPtySession（ローカルシェルPTY管理・CWD追跡）
    │   └── tmux_cc.rs            # TmuxControlSession（tmux -CC 制御モード用スケルトン）
    ├── vte/
    │   ├── mod.rs                # RenderableCell, RgbColor, CursorState
    │   └── vt100_adapter.rs      # Vt100Adapter（エミュレーション、スクロールバック、リサイズ時リフロー、256色/TrueColor変換）
    └── ui/
        ├── mod.rs
        ├── app.rs                # TerminApp（eframe::App 実装、X11PinchListener、カスタムウィンドウリサイズ枠）
        ├── scheduler.rs          # PollMode, RepaintScheduler（イベント駆動 / 連続FPSポーリング制御）
        ├── state.rs              # InteractionMode, PaneContext（モーダル・移動・ブロードキャスト等の状態判定）
        ├── workspace.rs          # Workspace, TreeBehavior（egui_tiles ツリー描画、アクションディスパッチ）
        ├── tree_ops.rs           # ペイン分割・タブ追加・削除・入れ替え・ブロードキャストグループ管理・フォーカス復旧
        ├── titlebar.rs           # Terminator風カスタムタイトルバーの描画とインタラクション
        └── pane.rs               # TerminalPane（メッシュキャッシュ描画、マウス選択、スクロールバー、キー入力処理）
```

---

## 3. コマンドライン引数・起動仕様 (`src/main.rs`)

### 3.1 CLI オプション一覧

| オプション | 引数 | 説明 |
| :--- | :--- | :--- |
| `-h`, `--help` | なし | ヘルプメッセージを表示して終了する |
| `-v`, `--version` | なし | バージョン情報 (`termin 0.0.3`) を表示して終了する |
| `-M`, `--maximize`, `-m`, `--maximise` | なし | 起動時にウィンドウを最大化する |
| `-d`, `--debug` (`-dd` 等の連結可) | なし | デバッグレベルを加算する（`1` 以上で標準エラー出力にログ出力、`2` 以上でデバッグサーバー起動） |
| `-g`, `--config <CONFIG>`, `--config=<CONFIG>` | パス | 読み書きする設定ファイルのパスを明示指定する |
| `-j`, `--config-json <JSON>`, `--config-json=<JSON>` | JSON文字列またはファイルパス | 既存の設定に対して JSON オブジェクトをディープマージして上書き適用する |
| `-e`, `--command ...`, `-x`, `--execute ...` | コマンド引数群 | 起動時に初期ペインのシェルへ `-c` で渡すコマンドを指定する（`--command=...`, `--execute=...` も対応。複数引数は適切にシェルクォートして結合される） |
| `-f`, `--fps <FPS>`, `--fps=<FPS>` | `1..=240` | ターゲット FPS を上書きする（範囲外の値は `1..=240` にクランプ） |
| `--mode <MODE>`, `--mode=<MODE>` | `event` / `poll` | 再描画モードを指定する（`event`, `event-driven` または `fps`, `continuous`, `polling`, `poll`） |
| `--event`, `--event-driven` | なし | イベント駆動再描画モード (`PollMode::EventDriven`) を使用する |
| `--poll`, `--continuous` | なし | 連続 FPS ポーリングモード (`PollMode::ContinuousFps`) を使用する |
| `--prefs` | なし | 起動時に Preferences（環境設定）ウィンドウを開いた状態にする |

### 3.2 ポーリングモードの決定ロジック
- `--mode`（または `--event` / `--poll`）が明示指定されている場合はそのモードを採用する。
- モード未指定かつ `-f` / `--fps` が指定された場合は `PollMode::ContinuousFps` に自動的に切り替わる。
- いずれも指定されていない場合のデフォルトは `PollMode::EventDriven` となる。

### 3.3 デバッグサーバー (`debug_level >= 2`)
- `-dd`（または `-d -d`）指定時、環境変数 `TERMIN_DEBUG_ADDR`（未設定時は `"127.0.0.1:0"`）で TCP リスナーを別スレッド (`termin-debug-server`) にて起動する。
- バインド成功時、標準エラー出力に `[termin][debug] Debug server listening on <addr>` を出力する。
- クライアント接続時に以下の JSON 1行を返してストリームを閉じる：
  ```json
  {"program":"termin","version":"0.0.3","pid":12345,"cli":"CliOptions { ... }"}
  ```

### 3.4 ウィンドウ初期設定
- タイトル: `"termin"`
- OS 標準ウィンドウ装飾 (`decorations`): `false`（フレームレス）
- 初期ウィンドウ内部サイズ: `1100.0 x 720.0`（最小内部サイズ: `400.0 x 260.0`）
- `egui` 標準の終了ショートカット (`quit_shortcuts`) およびキーボードズーム (`zoom_with_keyboard`) は無効化し、すべてアプリケーション側のショートカット設定で制御する。

---

## 4. 設定・テーマ・ショートカット仕様 (`src/config/theme.rs`)

### 4.1 設定ファイルのパスとロード順序
1. `-g` / `--config` でカスタムパスが指定されていればそのパスを使用する。
2. 未指定時は `~/.config/termin/config.json` を使用する。
3. 読み込み時に `~/.config/termin/config.json` が存在しない場合、レガシーパス `~/.config/termini/config.json` からの読み込みを試みる。
4. `-j` / `--config-json` が指定されている場合、ロードされた設定に対して JSON パッチを再帰的にマージ (`merge_json_values`) する。
5. 設定ウィンドウ (`PreferencesWindow`) で値が変更されるたびに、即座に `config_path()` へ整形済み JSON として自動保存される。

### 4.2 `ThemeSettings` 設定項目一覧

| フィールド名 | 型 | デフォルト値 | 実効範囲 / フォールバック挙動 |
| :--- | :--- | :--- | :--- |
| `title_active_bg` | `String` | `""` | 空または無効時は `#752100` (`rgb(117, 33, 0)`) |
| `title_active_fg` | `String` | `""` | 空または無効時は `#ffffff` (`Color32::WHITE`) |
| `title_inactive_bg` | `String` | `""` | 空または無効時は `#4b4b4b` (`rgb(75, 75, 75)`) |
| `title_inactive_fg` | `String` | `""` | 空または無効時は `#d2d2d2` (`rgb(210, 210, 210)`) |
| `frame_bg_color` | `String` | `"#2b2b2b"` | 無効時は `rgb(43, 43, 43)`。`terminal_bg_opacity` のアルファ値が適用される |
| `frame_border_color` | `String` | `"#5a5a5a"` | 無効時は `rgb(90, 90, 90)` |
| `separator_color` | `String` | `""` | 空または無効時は `#4a4a4a` (`rgb(74, 74, 74)`) |
| `terminal_bg_color` | `String` | `"#000000"` | 無効時は `#000000` (`Color32::BLACK`) |
| `terminal_bg_opacity` | `f32` | `1.0` | `0.0..=1.0` にクランプ。ターミナル背景とフレーム背景の透明度を制御 |
| `separator_width` | `f32` | `2.0` | `1.0..=16.0` (px) にクランプ。ペイン境界線の太さ |
| `frame_border_width` | `f32` | `5.0` | `1.0..=20.0` (px) にクランプ。非最大化時のウィンドウ外周マージン幅 |
| `target_fps` | `u32` | `30` | `1..=240` (FPS) にクランプ（`0` の場合はロード時に `30` に補正） |
| `show_scrollbar` | `bool` | `true` | 各ペイン右端のスクロールバー表示有無 |
| `font_path` | `String` | 自動検出パス | Debian 系システムの日本語対応等幅フォントを自動検出 |
| `ui_font_size` | `f32` | `13.0` | `8.0..=32.0` (px) にクランプ（UIスライダー範囲は `9.0..=28.0`） |
| `terminal_font_size` | `f32` | `14.0` | `8.0..=48.0` (px) にクランプ（UIスライダー範囲は `8.0..=36.0`） |
| `language` | `String` | `"auto"` | `"auto"`, `"en"`, `"ja"`, または検出された `.po` 言語コード |
| `custom_po_path` | `String` | `""` | 任意の `.po` ファイルへのパス（指定時は `language` より優先） |
| `shortcuts` | `ShortcutSettings` | 下記参照 | キーボードショートカット設定 |

#### カラー文字列のパース仕様 (`parse_color`)
- `#rrggbb`（6桁の16進数、大文字・小文字両対応）
- `rgb(r, g, b)`（各成分 `0..=255` の10進数、空白許容）
- 上記以外の文字列や空文字列は `None` となり、各色のデフォルト値にフォールバックする。

#### ブロードキャストグループのタイトルバー背景色パレット
ブロードキャストグループ（`group_id >= 1`）に属するペインは、通常のタイトルバー背景色の代わりにグループ番号に応じた6色サイクルパレットを使用する：

| グループ番号 (`(group_id - 1) % 6`) | 色系統 | アクティブ時 RGB | 非アクティブ時 RGB |
| :--- | :--- | :--- | :--- |
| Group 1 (`idx = 0`) | Blue | `(25, 118, 210)` | `(21, 75, 135)` |
| Group 2 (`idx = 1`) | Emerald Green | `(16, 142, 92)` | `(18, 88, 60)` |
| Group 3 (`idx = 2`) | Purple | `(138, 58, 185)` | `(86, 40, 116)` |
| Group 4 (`idx = 3`) | Amber / Orange | `(217, 119, 6)` | `(138, 78, 12)` |
| Group 5 (`idx = 4`) | Cyan / Teal | `(0, 151, 167)` | `(10, 93, 104)` |
| Group 6 (`idx = 5`) | Magenta / Pink | `(194, 24, 91)` | `(120, 22, 62)` |

### 4.3 フォント管理仕様
1. **デフォルトフォント / CJK フォールバックの探索順 (`default_debian_cjk_fallback`)**:
   存在する最初のフォントファイルを採用する：
   1. `/usr/share/fonts/opentype/noto/NotoSansMonoCJKjp-Regular.otf`
   2. `/usr/share/fonts/truetype/vlgothic/VL-Gothic-Regular.ttf`
   3. `/usr/share/fonts/truetype/vlgothic/VL-Gothic-regular.ttf`
   4. `/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf`
   5. `/usr/share/fonts/truetype/ipafont-gothic/ipag.ttf`
   6. `/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf`
   7. `/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc`
   8. `/usr/share/fonts/truetype/fonts-japanese-gothic.ttf`
2. **フォントバイナリの検証 (`is_valid_font_bytes`)**:
   TrueType / OpenType のマジックバイト（`[0x00, 0x01, 0x00, 0x00]`, `"OTTO"`, `"true"`, `"typ1"`）および TrueType Collection (`"ttcf"`) のヘッダとテーブル数を検証し、破損・非対応ファイルによるパニックを防止する。
3. **システムフォント探索 (`discover_system_fonts`)**:
   `/usr/share/fonts`, `/usr/local/share/fonts`, `~/.local/share/fonts`, `~/.fonts` を再帰走査し、`.ttf`, `.otf`, `.ttc` ファイルを収集する。並び順は以下の優先度（`font_sort_priority`）に従う：
   - 優先度 0〜1: CJK 対応かつ等幅 (Monospace) フォント（Regular が 0、その他ウェイトが 1）
   - 優先度 2〜3: CJK 対応フォント（Regular が 2、その他が 3）
   - 優先度 4〜5: 等幅 (Monospace) フォント（Regular が 4、その他が 5）
   - 優先度 6: 主要欧文フォント（DejaVu, Liberation, Roboto, Lato, FreeSans/Serif, NotoSans/Serif）
   - 優先度 7: その他すべて

### 4.4 キーボードショートカット仕様 (`ShortcutSettings`)

| 設定キー | 機能 | デフォルトバインディング |
| :--- | :--- | :--- |
| `copy` | 選択範囲（または画面全体）のコピー | `"Ctrl+Shift+C"` |
| `paste` | クリップボードからペースト | `"Ctrl+Shift+V"` |
| `split_horizontal` | 水平分割（左右に分割） | `"Ctrl+Shift+E"` |
| `split_vertical` | 垂直分割（上下に分割） | `"Ctrl+Shift+O"` |
| `new_tab` | 新規タブ追加 | `"Ctrl+Shift+T"` |
| `close_pane` | アクティブペインを閉じる | `"Ctrl+Shift+W"` |
| `focus_next_pane` | 次のペインへフォーカス移動 | `"Ctrl+Tab"` |
| `focus_prev_pane` | 前のペインへフォーカス移動 | `"Ctrl+Shift+Tab"` |
| `open_preferences` | 環境設定ウィンドウを開く | `"Ctrl+,"` |
| `close_preferences` | 環境設定ウィンドウを閉じる | `"Ctrl+Shift+W"` |
| `toggle_collapse_preferences` | 環境設定ウィンドウの折りたたみ切り替え | `"Ctrl+Shift+M"` |
| `toggle_fullscreen` | ウィンドウ最大化トグル | `"F11"` |
| `zoom_in` | ペインの文字サイズ拡大 | `"Ctrl+="` |
| `zoom_out` | ペインの文字サイズ縮小 | `"Ctrl+-"` |
| `zoom_reset` | ペインの文字サイズ倍率リセット | `"Ctrl+0"` |

- **複数バインディングの指定**: `/`, `|`, または `,` 区切りで1つのアクションに複数のショートカットを登録可能（例: `"Ctrl+Shift+C / Super+C"`）。ただし `"Ctrl+,"` のように `+` の直後に来るカンマはキー自体として解釈し、区切り文字として分割しない。
- **修飾キー表記のエイリアス**:
  - Ctrl: `Ctrl`, `Control`, `Ctl`
  - Shift: `Shift`, `Shft`
  - Alt: `Alt`, `Option`, `Opt`
  - Super: `Super`, `Win`, `Windows`, `Meta`, `Cmd`, `Command`, `Logo`
- **Plus / Equals の同一視**: `zoom_in` 等でキーボード配列差を吸収するため、`Plus` (`+`) と `Equals` (`=`) は相互にマッチする。
- **Super キーの確実な検出 (`detect_super_held`)**: `egui` の修飾キーやキーイベントに加え、Wayland (XWayland) や X11 環境で `winit` が Super キーイベントを取りこぼすケースへの対策として、`x11rb` でルートウィンドウのポインタマスク (`Mod4Mask = 1 << 6`) を直接クエリするフォールバックを備える。

---

## 5. レイアウトプリセット仕様 (`src/config/layout_preset.rs`)

ワークスペースの分割状態およびタブ構成を JSON ファイル (`~/.config/termin/layout.json`) に保存・復元できる。

### 5.1 データ構造 (`LayoutNode`)
```rust
pub enum LayoutNode {
    Pane {
        title: String,
        command: Option<String>,
    },
    HorizontalSplit {
        children: Vec<LayoutNode>,
    },
    VerticalSplit {
        children: Vec<LayoutNode>,
    },
    Tabs {
        children: Vec<LayoutNode>,
    },
}
```
- **保存時 (`LayoutNode::from_tree`)**: 現在の `egui_tiles::Tree<TerminalPane>` を走査し、ペインのタイトルとコンテナ階層（`Horizontal`, `Vertical`, `Tabs`、および `Grid` は `HorizontalSplit` に変換）をスナップショット化して `~/.config/termin/layout.json` に書き出す。
- **復元時 (`LayoutNode::build_tree`)**: `layout.json` からツリーを再構築し、各ペインの PTY セッションを新規起動して最初のペインにフォーカスを設定する。

---

## 6. 国際化・多言語対応仕様 (`src/i18n/mod.rs`)

### 6.1 GNU gettext `.po` パーサー (`PoCatalog`)
- 外部クレートに依存せず、標準的な GNU gettext `.po` フォーマットを独自パーサーで解析する。
- サポートする構文:
  - ヘッダーメタデータ (`msgid ""` の `msgstr` 内の `Key: Value` 行)
  - コンテキスト (`msgctxt`)：内部キーを `"{msgctxt}\u{0004}{msgid}"` として保持
  - 単数形 (`msgid` / `msgstr`) および複数形 (`msgid_plural` / `msgstr[0]`, `msgstr[1]`, ...)
  - 複数行文字列の連結とエスケープシーケンス（`\n`, `\r`, `\t`, `\"`, `\\`）の展開
  - `#, fuzzy` フラグが付与された未確定エントリは自動的にスキップし、原文 (`msgid`) にフォールバックする。

### 6.2 カタログ解決の優先順位 (`load_catalog_for_language`)
1. `custom_po_path` が空でなく、ファイルが読み込み可能であれば最優先でパースして適用する。
2. `language` が `"auto"` または空の場合、環境変数 `LC_ALL` → `LC_MESSAGES` → `LANG` の順で検査し、`"C"` / `"POSIX"` 以外の値から言語コード（例: `"ja_JP.UTF-8"` → `"ja"`）を抽出する（未設定時は `"en"`）。
3. 以下のディレクトリから `<lang>.po` ファイルを探索 (`discover_po_files`) し、見つかれば読み込む：
   - `./po`
   - 実行ファイルと同じディレクトリの `po/`、および親ディレクトリの `po/`
   - `~/.config/termin/po`
4. 外部ファイルが見つからない場合、バイナリに `include_str!` で埋め込まれた `po/ja.po`（言語コードが `"ja"` で始まる場合）または `po/en.po` にフォールバックする。

---

## 7. コンテキストメニュー仕様 (`src/menus/`)

### 7.1 アーキテクチャ
- `MenuManager` はターゲット種別 (`MenuTargetKind::Pane` / `MenuTargetKind::Titlebar`) ごとにメニュー項目を生成・描画する。
- `register_plugin_entry(target, entry)` により外部・プラグイン用のメニュー項目 (`MenuAction::PluginCommand(String)`) を動的に追加可能な設計となっている。
- `MenuEntry` は `Item`（アイコン・ラベル・ショートカット表記・アクション・有効フラグ）、`SubMenu`（任意の深さにネスト可能なサブメニュー）、`Separator`（区切り線）の3種類で構成される。

### 7.2 デフォルトメニュー構成

#### ペイン右クリックメニュー (`MenuTargetKind::Pane`)
1. `📋 Copy` (ショートカット表示あり)
2. `📥 Paste` (ショートカット表示あり)
3. `---` (Separator)
4. `🪟 Split Pane` (サブメニュー)
   - `↔ Split Horizontally`
   - `↕ Split Vertically`
5. `➕ New Tab`
6. `🧹 Clear Scrollback`
7. `---` (Separator)
8. `⚙ Open Settings`
9. `✖ Close Pane`

#### タイトルバー右クリックメニュー (`MenuTargetKind::Titlebar`)
1. `➕ New Tab`
2. `🪟 Split Pane` (サブメニュー)
   - `↔ Split Horizontally`
   - `↕ Split Vertically`
3. `💾 Layout` (サブメニュー)
   - `💾 Save Current Layout`
   - `📂 Load Saved Layout`
4. `---` (Separator)
5. `⛶ Toggle Fullscreen`
6. `⚙ Open Settings`
7. `---` (Separator)
8. `✖ Close Pane`

---

## 8. セッション・PTY 管理仕様 (`src/session/`)

### 8.1 `SessionDriver` トレイト
すべてのターミナルセッションは以下の共通インターフェースを実装する：
- `read_output(&mut self) -> Vec<u8>`: 非ブロッキングで受信済み出力バイト列を取得する。
- `write_input(&mut self, data: &[u8])`: ユーザーのキー入力やペーストデータを書き込む。
- `resize(&mut self, rows: NonZeroU16, cols: NonZeroU16)`: 端末の行数・列数を変更する。
- `is_alive(&mut self) -> bool`: セッション（子プロセス）が生存しているか判定する。
- `current_working_dir(&self) -> Option<PathBuf>`: 現在の作業ディレクトリを取得する。

### 8.2 `LocalPtySession` (`src/session/local_pty.rs`)
- **シェル起動**:
  - 環境変数 `SHELL`（未設定時は `"/bin/bash"`）を起動する。
  - 起動コマンド (`startup_command`) が指定されている場合は `<SHELL> -c "<startup_command>"` として実行する。
  - 作業ディレクトリ (`working_dir`) が指定されている場合は `cwd` および環境変数 `PWD` に設定する。
  - 環境変数として `TERM=xterm-256color` および `COLORTERM=truecolor` を設定する。
- **非同期リーダーとイベント駆動 Wakeup**:
  - 専用バックグラウンドスレッドで PTY マスターから `8192` バイト単位で `read` を行う。
  - 読み取ったバイト列は `mpsc::channel` で UI スレッドへ送信する。
  - `wakeup_pending: Arc<AtomicBool>` フラグを用い、UI スレッドが `read_output()` でデータを取り出すまでの間に **1回だけ** `egui::Context::request_repaint()` を呼び出すことで、大量出力時の過剰な再描画通知を抑制している。
  - EOF またはエラーでリーダーが終了した際、`exited` フラグを `true` にセットし、即座に `request_repaint()` を送ってペイン自動クローズを促す。
- **カレントディレクトリの動的追跡 (`current_working_dir`)**:
  - Unix 環境では PTY マスターのフォアグラウンドプロセスグループリーダー (`master.process_group_leader()`) の PID から `/proc/<pid>/cwd` のシンボリックリンク先を読み取る。
  - 取得できない場合は子シェル自身の `/proc/<child_pid>/cwd`、それも不可なら初期作業ディレクトリ (`initial_cwd`) にフォールバックする。
  - ペイン分割時や新規タブ追加時、この仕組みによってアクティブペインの現在のディレクトリが新しいペインへ自動的に引き継がれる。

### 8.3 `TmuxControlSession` (`src/session/tmux_cc.rs`)
- 将来の `tmux -CC`（コントロールモード）統合に向けた `SessionDriver` のスケルトン実装である。

---

## 9. VTE・ターミナルエミュレーション仕様 (`src/vte/`)

### 9.1 `Vt100Adapter` の基本仕様
- 内部パーサーとして `vt100::Parser` を保持し、最大スクロールバック行数 (`SCROLLBACK_LEN`) は **`5000` 行** に設定されている。
- デフォルト前景色は `RgbColor(230, 230, 230)`、デフォルト背景色は `RgbColor(18, 18, 18)`。
- 出力バイト処理 (`process_bytes`) のたびに世代カウンタ (`generation: u64`) をインクリメントし、UI 側の行キャッシュ・メッシュ再構築判定に使用する。
- ブラケットペーストモード (`bracketed_paste_enabled`) の状態取得に対応する。

### 9.2 リサイズ時の論理行リフロー機能 (`resize`)
ウィンドウリサイズやペイン分割によって列数・行数が変化した際、以下の仕様で表示内容を再整形（リフロー）する：
1. **代替画面バッファ (`alternate_screen`) 判定**: `vim`, `less`, `htop` 等のフルスクリーンアプリ実行中（代替画面有効時）はリフローを行わず、`set_size` のみ実行する。
2. **論理行の抽出 (`extract_all_logical_lines`)**:
   - スクロールバック履歴の最古行から現在の画面の最終アクティブ行までを順に走査する。
   - 右端で自動折り返しされた行 (`screen.row_wrapped(r) == true`) は改行を入れずに次の物理行と結合し、1つの「論理行」として復元する。
   - 折り返されていない行の末尾にある空白セル（デフォルト背景・非反転）はトリムする（ただしカーソル行ではカーソル列までの空白を保持する）。
3. **ANSI シーケンスへの再シリアライズ (`serialize_cells_to_ansi`) と再投入**:
   - 抽出した各論理行のセルが持つ文字・前景色・背景色・太字 (`\x1b[1m`)・下線 (`\x1b[4m`)・反転 (`\x1b[7m`) を ANSI エスケープシーケンス付きのバイト列に復元し、新しい列幅・行数で作られた `vt100::Parser` に再処理させることで、新しい幅に応じた自然な折り返しとスクロールバック位置を実現する。
4. **スクロールバック消去 (`clear_scrollback`)**:
   - 現在のライブ画面の論理行のみを抽出して新規パーサーに再投入することで、現在の画面表示を維持したまま過去のスクロールバック履歴だけをクリアする。

### 9.3 カラーパレット変換 (`ansi_idx_to_rgb`)
`vt100::Color` を 24bit `RgbColor` に変換する：
- **Index 0..=15（標準16色・One Dark 風視認性重視パレット）**:
  - `0` (Black): `(40, 44, 52)` / `8` (Bright Black): `(92, 99, 112)`
  - `1` (Red): `(224, 108, 117)` / `9` (Bright Red): `(244, 128, 137)`
  - `2` (Green): `(152, 195, 121)` / `10` (Bright Green): `(172, 215, 141)`
  - `3` (Yellow): `(229, 192, 123)` / `11` (Bright Yellow): `(249, 212, 143)`
  - `4` (Blue): `(97, 175, 239)` / `12` (Bright Blue): `(117, 195, 255)`
  - `5` (Magenta): `(198, 120, 221)` / `13` (Bright Magenta): `(218, 140, 241)`
  - `6` (Cyan): `(86, 182, 194)` / `14` (Bright Cyan): `(106, 202, 214)`
  - `7` (White): `(220, 223, 228)` / `15` (Bright White): `(255, 255, 255)`
- **Index 16..=231**: 6×6×6 の 216色 RGB キューブ（各成分 `0` または `55 + v * 40`）
- **Index 232..=255**: 24段階グレースケール（`8 + (idx - 232) * 10`）
- **TrueColor (`vt100::Color::Rgb(r, g, b)`)**: そのまま `(r, g, b)` として描画する。

---

## 10. UI・ワークスペース・インタラクション仕様 (`src/ui/`)

### 10.1 再描画スケジューラ (`src/ui/scheduler.rs`)
- **`PollMode::EventDriven`（デフォルト）**:
  - PTY リーダースレッドからの `request_repaint()` またはユーザーのマウス・キー入力があった時のみフレームを更新する。
  - PTY 出力が連続している間は、`target_fps` から算出されるフレーム間隔（`1000 / target_fps` ms、最小 4ms）でスロットリングしながら再描画をスケジュールし、CPU 負荷を抑える。
- **`PollMode::ContinuousFps`**:
  - PTY スレッドからの強制 Wakeup を行わず、常時 `1000 / target_fps` ms 間隔で `request_repaint_after` を呼び出し続ける。
- ウィンドウ最小化時 (`is_minimized == true`) は次フレームの再描画スケジュールを停止する。

### 10.2 X11 連携とフレームレスウィンドウ制御 (`src/ui/app.rs`)
1. **カスタムリサイズ枠 (`handle_custom_resize_frame`)**:
   - フレームレスウィンドウのため、非最大化時にウィンドウ外周の `effective_border_width() + 2.0` px 以内にマウスホバーするとリサイズカーソルに変更し、左ドラッグ開始で `ViewportCommand::BeginResize(ResizeDirection)` を発行する。
   - 四隅 `16.0` px (`RESIZE_CORNER_SIZE`) 以内は斜めリサイズ（`NorthWest`, `NorthEast`, `SouthWest`, `SouthEast`）、辺上は上下左右リサイズ（`North`, `South`, `West`, `East`）となる。
2. **XInput 2.4 タッチパッドピンチズーム (`X11PinchListener`)**:
   - X11 / XWayland ウィンドウハンドルから XInput 2.4 の `GesturePinchBegin` (27), `GesturePinchUpdate` (28), `GesturePinchEnd` (29) イベントを購読し、33ms 周期でポーリングしてアクティブペインのズーム倍率 (`zoom_scale`) に反映する。
   - ウィンドウ非フォーカス時はシステムコール発行（ポーリング）を完全に停止する。
3. **起動時ウィンドウフォーカス保証 (`ensure_startup_focus`)**:
   - 起動直後の数フレーム間、ウィンドウが Viewable になったタイミングで `_NET_ACTIVE_WINDOW` クライアントメッセージ送信および `set_input_focus` を実行し、起動直後から即座にキー入力を受け付ける。

### 10.3 ワークスペースの操作モード (`InteractionMode` / `PaneContext`)
ワークスペースは以下の5つの排他的な `InteractionMode` を持つ：

| モード | 説明・遷移条件 |
| :--- | :--- |
| `Normal` | 通常のターミナル操作モード。ペイン境界線のドラッグリサイズが可能 |
| `MovingPane(TileId)` | タイトルバー左端の `[ ✥ ]` ボタンを**左クリック**した状態。移動元ペインが黄色枠 (`rgb(250, 204, 21)`) でハイライトされ、次にクリックしたペインと位置を入れ替える (`swap_panes`)。同じペインの再クリックまたは `Esc` キーで解除 |
| `SelectingBroadcast(TileId)` | タイトルバー左端の `[ ✥ ]` ボタンを**右クリック**した状態。選択元ペインが水色枠 (`rgb(56, 189, 248)`) でハイライトされる。<br>・別のペインをクリックすると同一ブロードキャストグループに紐付ける (`link_broadcast_panes`)。<br>・`Ctrl`（または `Command`）を押しながらクリックすると選択モードを維持したまま複数ペインを連続追加できる。<br>・自分自身のペインをクリックするとブロードキャストグループから離脱 (`leave_broadcast_group`) する。<br>・グループの所属ペインが1つ以下になった場合は自動的にグループ解除される (`cleanup_singleton_broadcast_groups`)。 |
| `EditingTitle(TileId)` | タイトルバーのタイトル文字列をダブルクリックしてインライン編集中の状態。他ペインの操作はブロックされる。`Enter`、`Esc`、またはフォーカス喪失で確定する |
| `BlockedByModal` | 環境設定ウィンドウ (`PreferencesWindow`) やフォントサンプルウィンドウ上にマウスポインタがある、または設定画面でテキスト入力・ショートカット録画中の状態。背後にあるターミナルペインへの誤入力やクリック貫通を完全に遮断する |

### 10.4 カスタムタイトルバー (`src/ui/titlebar.rs`)
各ペインの上部に高さ `24.0` px の Terminator 風タイトルバーを描画する：
- **左側エリア**:
  - `[ ✥ ]` グリップボタン：左クリックでペイン移動モード、右クリックでブロードキャスト選択モードを開始・トグルする。
  - ブロードキャストグループバッジ：グループ所属時のみ `[ 1 ]` のようなグループ番号バッジを表示する。
- **中央エリア**:
  - ペインタイトル（初期値は `$SHELL` または `"/bin/bash"`）を中央揃えで表示する。
  - タイトル文字列上をダブルクリックするとインラインテキスト編集 (`TextEdit`) に切り替わる。
  - タイトル文字列以外の余白部分をダブルクリックするとウィンドウ最大化・復元 (`ToggleMaximizeWindow`) を実行する。
  - 中央エリアを左ドラッグすると OS ウィンドウ自体の移動 (`ViewportCommand::StartDrag`) を開始する。
- **右側エリア**:
  - `⚙` ボタン：Preferences ウィンドウを開く。
  - `—`（最小化）ボタン / `🗖`（最大化トグル）ボタン：**ワークスペース内で最も右上にあるペイン (`is_top_right == true`) のタイトルバーにのみ**表示される。
  - `×` ボタン：そのペインを閉じる (`ClosePane`)。最後の1ペインが閉じられた場合はアプリケーション全体を終了する。

### 10.5 ターミナルペインの描画・入力・選択仕様 (`src/ui/pane.rs`)

#### 1. 行単位ハッシュキャッシュと事前テッセレーション描画
- 画面のグリッドサイズ・位置・フォントサイズ (`GridCacheMeta`)、VTE の世代 (`generation`)、スクロール位置 (`scroll_offset`)、選択範囲 (`norm_selection`) のいずれかが変化した時のみ描画データを更新する。
- 各行 (`0..rows`) ごとにセル内容（文字、前景色、背景色、太字、下線、反転、全角フラグ）と選択範囲のハッシュ値 (`content_hash`) を計算し、変化した行のみ背景矩形・下線 (`bg_and_lines`) と文字ギャレー (`text_shapes`) を再生成する。
- 全行の背景矩形と文字シェイプを1つの `egui::Mesh` (`cached_terminal_mesh`) に事前テッセレーション・結合して保持し、変化がないフレームでは `Arc<egui::Mesh>` をクローンして1回のドローコールで高速描画する。
- 太字 (`bold`) セルは前景色 RGB の各成分に `+25`（飽和加算）して視認性を高める。
- カーソル描画：アクティブペインでは半透明矩形塗りつぶし (`rgba(220, 220, 220, 160)`)、非アクティブペインでは中抜き枠線 (`rgb(160, 160, 160)`) で表示する（全角セル上では2カラム幅に自動拡張）。

#### 2. スクロールバックとテキスト選択・コピー
- **絶対行ベース選択 (`abs_selection_start` / `abs_selection_end`)**:
  - スクロールバック先頭からの絶対行インデックスと列で選択範囲を管理する。
  - 選択中に新しい出力が流れてきたり、マウスホイールでスクロールしたり、画面上下端の外側へドラッグしてオートスクロールした場合でも、選択位置がテキストに追従する。
- **コピー動作 (`copy_selection_or_screen`)**:
  - 選択範囲が存在する場合は、スクロールバック領域にある行も含めて選択範囲のテキストを全文抽出 (`extract_full_selected_text`) し、各行末の余分な空白をトリムしてクリップボードにコピーする。
  - 選択範囲が存在しない場合は、現在画面に表示されている全行のテキストをコピーする。
- **スクロール操作とオフセット表示**:
  - マウスホイール / タッチパッドスクロール、右端スクロールバーのクリック・ドラッグ、およびキーボード操作により過去ログを最大 5000 行まで遡れる。
  - スクロール中（`scroll_offset > 0`）は、ペイン右上に黄色文字で `↑ -<現在オフセット>/<最大オフセット>` のインジケータバッジを表示する。
  - スクロール中に通常の文字キーを入力するかペーストを行うと、自動的に最新位置 (`scroll_offset = 0`) に戻ってから PTY へ送信する。

#### 3. ズーム機能
- ショートカットキー（`zoom_in`, `zoom_out`, `zoom_reset`）、ピンチジェスチャー、または **Super キーを押しながらマウスホイールスクロール** することで、ペインごとに独立してフォントサイズ倍率 (`zoom_scale`: `0.4..=3.5`、実効フォントサイズ `6.0..=64.0` px) を変更できる。
- ズームに応じて1文字のピクセル幅・高さが再計測され、ペインの行数・列数が自動的にリサイズ＆リフローされる。

#### 4. キーボード入力・制御シーケンス変換
ショートカットにマッチしなかったキー入力は、以下のようにバイト列へ変換されて PTY（および同一ブロードキャストグループ内の全ペイン）へ送信される：
- **Ctrl + 英字 (`A`..`Z`)**: `(ch as u8) & 0x1F`（`0x01`..`0x1A`）
- **Ctrl + 記号**: `Ctrl+Space` → `0x00`, `Ctrl+/` → `0x1F`, `Ctrl+[` → `0x1B`, `Ctrl+\` → `0x1C`, `Ctrl+]` → `0x1D`
- **Alt + 英数字**: `0x1B` + ASCII 小文字バイト
- **Shift + ナビゲーションキー**:
  - `Shift+PageUp` / `Shift+PageDown`: 画面行数の半分だけスクロールバックを上下移動
  - `Shift+Up` / `Shift+Down`: 1行ずつスクロールバックを上下移動
  - `Shift+Home` / `Shift+End`: スクロールバック最上部 / 最下部（最新位置）へジャンプ
  - `Shift+Insert`: クリップボードからペースト
  - `Shift+Tab`: `\x1b[Z` (Backtab)
- **単独の特殊キー**:
  - `Enter`: `\r`, `Backspace`: `0x7F`, `Tab`: `\t`, `Escape`: `0x1B`
  - 矢印キー: `Up` (`\x1b[A`), `Down` (`\x1b[B`), `Right` (`\x1b[C`), `Left` (`\x1b[D`)
  - `Home` (`\x1b[H`), `End` (`\x1b[F`), `Insert` (`\x1b[2~`), `Delete` (`\x1b[3~`), `PageUp` (`\x1b[5~`), `PageDown` (`\x1b[6~`)
  - ファンクションキー: `F1` (`\x1bOP`), `F2` (`\x1bOQ`), `F3` (`\x1bOR`), `F4` (`\x1bOS`), `F5` (`\x1b[15~`), `F6` (`\x1b[17~`), `F7` (`\x1b[18~`), `F8` (`\x1b[19~`), `F9` (`\x1b[20~`), `F10` (`\x1b[21~`), `F11` (`\x1b[23~`), `F12` (`\x1b[24~`)

---

## 11. 環境設定ウィンドウ仕様 (`src/config/prefs_ui.rs`)

### 11.1 ウィンドウ構造
- ウィンドウタイトル: `"Termin Preferences"`（翻訳対応）
- デフォルトサイズ: `760.0 x 520.0`（最小サイズ: `640.0 x 380.0`、リサイズ・折りたたみ可能）
- レイアウトは **固定ヘッダー（4つのタブバー）** ＋ **中央スクロール領域** ＋ **固定フッター** の3段構成。
- 固定フッターには以下のボタンを配置：
  - `Restore Defaults`: 全設定をデフォルト値にリセット
  - `Save Current Layout`: 現在のペイン・タブ構成を `~/.config/termin/layout.json` に保存
  - `Load Saved Layout`: 保存済みレイアウトを読み込んでワークスペースを再構築
  - `Close`: 設定ウィンドウを閉じる

### 11.2 各タブの機能
1. **`Titlebar & Frame` タブ**:
   - アクティブ/非アクティブ時のタイトルバー背景色・文字色、ウィンドウフレーム背景色・枠線色、ペイン境界線色、ターミナル背景色のテキスト入力＆カラーピッカー編集（適用状態/デフォルト/無効値のリアルタイムステータス表示付き）。
   - ターミナル背景不透明度 (`0.0..=1.0`)、ペイン境界線幅 (`1..=16` px)、ウィンドウ枠幅 (`1..=20` px)、ターゲット FPS (`1..=240` FPS) のスライダー＆ `−` / `+` ステップボタン調整、およびスクロールバー表示チェックボックス。
2. **`Font` タブ**:
   - システムフォントのキーワード絞り込み・再スキャン (`Rescan`)、コンボボックス選択、フォントファイルパスの直接入力。
   - UI・メニューフォントサイズ (`9.0..=28.0` px) およびターミナルフォントサイズ (`8.0..=36.0` px) の調整。
   - ASCII / 日本語テキストのライブプレビュー枠。
   - **`System Font Samples & Selector` サブウィンドウ**:
     - `Open Font Sample Catalog...` ボタンで開く別ウィンドウ。
     - カテゴリフィルタ（`All Fonts`, `Monospace`, `Japanese / CJK`）およびキーワード検索、任意のサンプル文字列編集に対応。
     - スクロール表示されている行のフォントのみを毎フレーム最大6件・最大ファイルサイズ 35MB・キャッシュ上限96件で遅延ロードし、各フォントの実際の字形でライブプレビューを描画する。`Use This Font` ボタンで即座に適用できる。
3. **`Shortcuts` タブ**:
   - 全15種類のショートカットに対するテキスト編集、`Record`（既存スロットの上書き録画）、`+ Add`（複数ショートカットの追加録画）、`✕`（個別バインディング削除）、`Default`（個別初期化）、および `Reset Shortcuts to Defaults`（一括初期化）。
   - 録画モード中は修飾キー単体では確定せず、通常キーが押された時点で `"Super+Ctrl+..."` 形式にフォーマットして登録する（`Esc` 単体押下で録画キャンセル）。
4. **`Language` タブ**:
   - `Auto (System Locale)`, `English (en.po)`, `日本語 (ja.po)`, および探索された外部 `.po` ファイルからの言語選択。
   - カスタム `.po` ファイルパスの直接指定と `Reload .po Files` ボタン。
   - 現在アクティブなカタログのロケール名と読み込まれた翻訳エントリ数のステータス表示。

---

## 12. テスト構成

各モジュール配下の `test/` ディレクトリにユニットテスト・統合テストが配置されている：
- `src/test/main_tests.rs`: CLI 引数パーサー、ヘルプ・バージョン出力、クォート処理等のテスト
- `src/config/test/prefs_ui_tests.rs`: カラーパース、ショートカット判定、設定UI・フォント検証のテスト
- `src/config/test/layout_preset_tests.rs`: レイアウトツリーのシリアライズ・復元テスト
- `src/i18n/test/i18n_tests.rs`: `.po` パーサー、複数形、fuzzy スキップ、言語検出のテスト
- `src/menus/test/renderer_tests.rs`: コンテキストメニュー構造・プリセットのテスト
- `src/session/test/local_pty_tests.rs`: ローカル PTY の入出力・コマンド実行・CWD 取得のテスト
- `src/session/test/tmux_cc_tests.rs`: `TmuxControlSession` の基本動作テスト
- `src/vte/test/vt100_adapter_tests.rs`: VT100 出力処理、スクロールバック、リサイズ時の論理行リフローのテスト
- `src/ui/test/pane_tests.rs`: ペイン描画・選択・コピー・ズーム・キー入力のテスト
- `src/ui/test/titlebar_tests.rs`: タイトルバーボタン・ダブルクリック・移動/ブロードキャストモード切替のテスト
- `src/ui/test/state_tests.rs`: `InteractionMode` および `PaneContext` の状態遷移・ブロック判定テスト
- `src/ui/test/scheduler_tests.rs`: `PollMode` と `RepaintScheduler` のテスト
