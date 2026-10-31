# Living Wallpaper — Windows MVP

群れモデルの動きをデスクトップ背景に表示する、Rust + wgpu製の壁紙アプリです。
現在は最初のモデルとして魚の群泳（Boids）を実装しています。今後、鳥・昆虫などの群れモデルを追加できる構成です。
対象はWindows 11 x64、プライマリ画面1枚です。

このリポジトリはソースコード配布用です。実行ファイル・ビルド環境・個人設定はGitの管理対象に含みません。
実行ファイルを利用する場合は、GitHubのReleasesから `living-wallpaper-windows-x64.zip` をダウンロードして展開してください。Release公開後にビルドが成功するとAssetsに添付されます。ソースから作る場合は下記「ソースからビルド」へ進んでください。

## 起動

ビルド後の `target\release\living-wallpaper.exe` を起動すると壁紙として動作します。
GitHub Releasesの実行用ZIPでは、展開したフォルダー直下の `living-wallpaper.exe` を起動してください。
通知領域のアイコン（隠れている場合は「^」内）を左／右クリックすると設定ポップアップが開きます。
MVPでは専用の設定ウィンドウではなく、Windows標準のポップアップメニューを採用しています。表示言語は英語です。

```powershell
.\target\release\living-wallpaper.exe
.\target\release\living-wallpaper.exe --preview
.\target\release\living-wallpaper.exe --preview --smoke-test
```

- Fish: 120 / 240 / 500 / 1000、FPS: 15 / 30 / 60
- Mouse interaction: マウス停止10秒後から、カーソルの42〜200物理pxの範囲で接近、移動速度35物理px/秒超は逃避。静止10秒後はカーソル周囲の半径24物理pxを簡易的な衝突領域として回り込む。ON/OFF可。
- Avoid desktop icons: アイコンとラベルの矩形を約2秒ごとに取得し回避。
- Power saving: バッテリー駆動時は最大15 FPS。ロック・スリープ中、壁紙対象画面が前面の全画面ウィンドウに覆われる場合は描画・群泳更新を停止。
- Pause: 手動停止。Quit: 終了。プレビューはEscまたは閉じるボタンでも終了。

Power savingの停止中も状態確認のため約2 Hzで起床します。0 CPUを保証するものではありません。
FPSは上限です。処理時間、GPUやディスプレイの制約によって実FPSは下がります。
通常の前面ウィンドウによる部分的な遮蔽の判定はしません。
プレビューはマウスと群泳・描画の確認用で、アイコン回避と全画面による自動停止を行いません。

設定: `%LOCALAPPDATA%\LivingWallpaper\settings.json`

ログ: `%LOCALAPPDATA%\LivingWallpaper\app.log`

旧Fish School Wallpaperの設定が存在し、新しい保存先に設定がまだない場合は旧設定を読み込みます。次に設定を変更すると新しい保存先へ保存します。旧設定ファイルは保持します。

設定は操作ごとに保存し、次の起動時に読み込みます。外部通信、マウス履歴の保存、自動起動登録、管理者権限要求は実装していません。
終了時にはアプリの壁紙ウィンドウが破棄され、通常のWindows壁紙が再び見えます。元の壁紙設定は書き換えません。

## ソースからビルド

推奨: Rust stable x64 MSVC、Visual Studio 2022以降の「C++によるデスクトップ開発」とWindows 11 SDK。
[Rust公式インストーラー](https://www.rust-lang.org/tools/install)と[MicrosoftのC++ビルド環境](https://learn.microsoft.com/en-us/cpp/build/vscpp-step-0-installation)を用意し、Developer PowerShellで実行します。

```powershell
cargo test --locked
cargo build --release --locked
.\target\release\living-wallpaper.exe --preview
.\target\release\living-wallpaper.exe
```

または `powershell -File .\build.ps1`。依存バージョンは `Cargo.lock` で固定。
`build.ps1` はビルド時のローカルパスを置換し、生成したEXEを検査してから `dist\` へ実行ファイル・README・ライセンス情報をまとめます。公開・再配布にはこの出力を使用してください。通常の `cargo build` の出力は、ローカルパスの検査を通していません。
既存の `dist\` がある場合は、混在を避けるため別の場所に保管してから再実行します。追加のコンパイラー引数は `-AdditionalRustFlags` または `CARGO_ENCODED_RUSTFLAGS` を使用します。
GitHub Actionsでも整形確認・テスト・Releaseビルドを実行し、同じ配布物をArtifactsとして保存します。CIの実行状況はGitHubへpushした後にActions画面で確認できます。
このMVPは既知のAPIに合わせwgpu 0.19.4、winit 0.29.15を採用しています。最新版追従は別途移行作業が必要です。
GNUターゲットも検証対象です。GNUではRust GNUツールチェーンに加えて完全なMinGW（as/dlltool/ld等）が必要になる場合があります。

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal
# 完全なMinGWのbinをPATHに追加してから:
cargo +stable-x86_64-pc-windows-gnu test --locked
cargo +stable-x86_64-pc-windows-gnu build --release --locked
```

GPU起動確認は `--smoke-test` で約8秒後に自動終了しログへ描画フレーム数を記録します。エラー時は終了コード1。
`--preview --smoke-test` で描画のみ、`--smoke-test` で実壁紙を確認できます。通常モードの二重起動は防止しています。
シミュレーション計測: `cargo test --release --locked simulation_benchmark -- --ignored --nocapture`。

## 対応範囲・制約

- 壁紙配置は `WorkerW` / `Progman`、非公開メッセージ `0x052C` などのExplorer内部構造に依存しています。Windows更新や構成差で動作しなくなる可能性があります。非対応時はエラーで停止し、`--preview` で群泳と描画を確認できます。
- アイコン回避もExplorerの内部コントロールに依存します。取得拒否・失敗時はログに残し、群泳を続けます。大量アイコンでは取得できた分だけを回避します。
- 混在DPI、画面構成変更、他GPU、HDR等は実機検証が必要です。全画面判定は前面ウィンドウの矩形による推定です。
- Explorer再起動で壁紙ホストが失われた場合は終了します。アプリを再起動してください。
- 現在は魚モデルのみ。複数モニターへの同時表示、モデル選択UI、自動起動登録、インストーラー、署名は未実装です。
- 省電力制御の分岐は実装していますが、電池消費・実機の全状態切替を保証するものではありません。

## macOS

共通シミュレーションとwgpu/winit部分はOSから分離しましたが、macOSでのビルド・実行は未検証です。
`--preview` のコード経路はあります。壁紙配置、アイコン取得、グローバルマウス、トレイ、電源／ロック通知はmacOSでは未実装のスタブです。
macOS対応にはAppKitのウィンドウレベル・画面座標変換・メニューバー・電源通知等の別バックエンドが必要です。macOS対応済み製品として扱わないでください。

ソースはMITライセンス。依存ライブラリの権利は各著作者に帰属します。
