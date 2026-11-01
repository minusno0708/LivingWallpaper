# Living Wallpaper — 開発引き継ぎ

このファイルはリポジトリ全体に適用するCodex向けの入口です。会話履歴や別PCの作業フォルダーを前提にせず開発してください。現在のユーザー指示を優先し、仕様・実装・検証記録を区別してください。

## プロジェクトと合意事項

- 自然・群れモデルの壁紙アプリ。パッケージ名・ディレクトリ名は `living-wallpaper`、アプリ表示名は `Living Wallpaper`、GitHubリポジトリ名は `LivingWallpaper`。
- Windows 11を主対象に、将来macOSにも対応できる構造にする。現在のmacOS連携は未実装。
- Rust + wgpuを共通基盤とし、低負荷を優先する。最初のモデルは魚のBoids群泳。魚以外を実装済みとして扱わない。
- デスクトップアイコンを障害物として回避する。マウスは停止時に接近、移動時に逃避し、影響をON/OFFできる。
- 常駐動作を基本とし、トレイから小さな設定ポップアップを開く。Windows標準コントロールの小さな設定ウィンドウを使い、項目変更では閉じない。右上の×・Escは画面を隠すだけで、アプリ終了と区別する。
- FPS上限、0 FPSによる手動停止、バッテリー・全画面・ロック等に応じた省電力制御を維持する。

## 最初に読む資料

1. [docs/codex/README.md](docs/codex/README.md): 資料の一覧と用途。
2. [docs/codex/requirements.md](docs/codex/requirements.md): 合意済み要件と受入条件。
3. [docs/codex/status.md](docs/codex/status.md): 実装済み・未実装・未検証の区分。

設計変更時は [architecture.md](docs/codex/architecture.md)、別PCでのセットアップ時は [development.md](docs/codex/development.md) と [git-setup.md](docs/codex/git-setup.md)、検証時は [validation.md](docs/codex/validation.md) を参照してください。資料を更新する前に実コードとGitの状態を確認し、古い記録を現在の実行結果として流用しないでください。

`docs/local/README.md` が存在する場合は、そのマシンだけの補足資料として参照してください。端末情報・個人情報・個人向け指示は `docs/local/` に保存し、共有資料へ転記しません。このディレクトリはGit管理対象外です。存在しない場合も共通資料だけで開発できるようにし、`git add -f` などで除外を解除しないでください。ローカル資料から得た情報をコミット本文や公開ログへ転載しないでください。

## 実装の境界

- `src/scene_time.rs`: 全シーン共通の時間倍率と時計。モデルと背景は同じ時間を使い、マウスやOSの状態確認は実時間を維持する。
- `src/simulation.rs`: OS非依存のモデルとBoids。`SchoolModel` は現状 `Fish` を返すため、完全にモデル中立な描画契約ではない。
- `src/renderer.rs`, `src/fish.wgsl`, `src/background.wgsl`: wgpuによる魚のインスタンス描画と深海・マリンスノーの背景描画。シミュレーションはCPU、描画はGPU。
- `src/windows_panel.rs`: Deep Glass設定画面のGDI描画。標準コントロールの値・フォーカスを使い、スライダーは独自入力と描画で滑らかに追従する。操作後に閉じない動作を維持する。
- `src/platform.rs`, `src/windows.rs`: OS境界とWindows連携。Windows依存を共通シミュレーションへ持ち込まない。
- `src/settings.rs`: 設定の範囲制限・保存。新設定がない場合の旧 `FishSchoolWallpaper` 設定読込を維持する。
- `src/main.rs`: イベントループ、描画制限、マウス入力、停止・復帰制御。

WorkerW / Progman / `0x052C` / ExplorerのListView構造への依存は非公開・不安定です。公開された壁紙APIやmacOS対応済みの実装と説明しないでください。アイコン取得でタイムアウトしたリモートバッファを即解放すると、Explorerの遅延書込先を失います。取得停止とバッファ保持の理由を理解してから変更してください。

## 開発・確認

WindowsではRust stable x64 MSVCとC++デスクトップ開発環境・Windows SDKを基本にします。GNU環境の再現手順は開発資料にあります。端末固有の絶対パスや、ローカル専用のビルド補助ツールをプロジェクトの必須条件にしないでください。

```powershell
cargo fmt --check
cargo test --locked
cargo build --release --locked
```

- Rust変更時は変更内容に応じて上記を実施する。モデル・GPUの変更には関連する既存テストを使う。
- 文書だけの変更でGPU・実壁紙の再検証を要求しない。文書移動時は相対リンク、ビルド設定、配布物を確認する。
- プレビュー確認は `target/release/living-wallpaper.exe --preview --smoke-test`。約8秒の描画成功は、アイコンの前後関係・マウス操作・電池消費の合格を意味しない。
- 未実行項目は未検証と記録し、過去の測定値を新しいPCの性能値として扱わない。
- 依存更新は対象タスクに必要な範囲で行い、`Cargo.lock` とソースのAPI整合性を確認する。

## 文書・成果物

`README.md` は利用・ビルドに必要な説明用。要件、設計判断、検証履歴、引き継ぎ、Git運用メモは `docs/codex/` にまとめ、変更時に関係する資料を更新してください。このディレクトリはGitで追跡し、別PCにも引き継ぎます。独自の自動読込機能はないため、この入口のリンクから必要な資料を読んでください。

`AGENTS.md` と `docs/codex/` はアプリの実行時依存ではなく、`dist/` へ同梱しません。実行ファイル、ツールチェーン、個人設定、ログ、認証情報もコミットしません。`third-party-licenses/` とライセンス表示は維持してください。

実行用配布物は `build.ps1` のパス置換・バイト検査を通したものを使う。通常の `cargo build` の出力やPDB・Cargoの中間ファイルをそのまま公開しない。公開履歴へ個人名・連絡先・端末構成・会話固有の好みを追加しない。

## Git

通常のブランチは `main`。送信先は `git remote -v` で確認し、アカウント名や認証情報をソースへ固定しない。作業開始時に未コミット変更を確認し、ユーザーの変更を保護する。公開するコミット情報・ソース・配布物に個人情報を含めない。`docs/codex/git-setup.md` を参照する。
