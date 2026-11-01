# 別PCでの開発・検証

## 取得とCodexの起点

```powershell
git clone <repository-url> living-wallpaper
cd living-wallpaper
git status --short --branch
```

取得先への認証が必要な場合は、各マシンで用意する。プロジェクトに認証情報を保存しない。

cloneした `Cargo.toml` と `AGENTS.md` のあるフォルダーをCodexのプロジェクトとして開く。ルートの `AGENTS.md` とこの資料群がGitに含まれるため、過去のチャットや元PCの絶対パスは不要。独自設定で別の指示ファイルが優先される場合は、実際の読込対象を確認する。

## Windowsの推奨環境

- Windows 11 x64。
- Rust stableの `x86_64-pc-windows-msvc`、rustfmt。
- Visual Studio 2022以降またはBuild Toolsの「C++によるデスクトップ開発」とWindows 11 SDK。
- wgpuが利用できるGPUとドライバー。GPU未接続のCIでGUI検証は行わない。

MSVCのリンカーが見つからない場合はDeveloper PowerShell等の環境設定とSDKの導入を確認する。依存パッケージは `Cargo.lock` に固定してあり、初回はダウンロードが必要。取得済みなら `--offline --locked` も利用できる。

```powershell
cargo fmt --check
cargo test --locked
cargo build --release --locked
```

基本の配布作成は `powershell -File ./build.ps1`。このスクリプトはテストとReleaseビルドに加え、コンパイラーのパス置換とEXEのローカルパス残存検査を行う。成功した場合だけ `dist/` に実行ファイル、README、ライセンス類をコピーする。Codex資料は含めない。通常の `cargo build` の生成物をそのまま公開しない。

既存の `dist/` がある場合は別の場所に保管してから実行する。`RUSTFLAGS` を使っている場合は引数境界の誤解釈を避けるため、設定を `CARGO_ENCODED_RUSTFLAGS` または `-AdditionalRustFlags` に移して `RUSTFLAGS` を未設定にする。元の環境変数はビルド後に復元される。

`CARGO_TARGET_DIR` を指定した場合はその出力先から実行ファイルを取得する。相対値はプロジェクトルートを起点に考え、マシン固有の設定は環境変数等で与える。元PCで使ったリポジトリ外の `work/` ツールチェーンを必要条件にしない。

## GNU環境を使う場合

当初のローカル検証はRust 1.99.1 GNUとLLVM-MinGW 20260616で実施した。MSVCの代替例であり、全マシンにこの構成を強制するものではない。

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal
$env:PATH = 'C:\build-tools\llvm-mingw\bin;' + $env:PATH
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = 'C:\build-tools\llvm-mingw\bin\x86_64-w64-mingw32-gcc.exe'
$env:RUSTFLAGS = $null
./build.ps1 -Toolchain stable-x86_64-pc-windows-gnu -AdditionalRustFlags @('-C', 'link-self-contained=yes')
```

上記のツールのパスは置換する。古いGNUリンカーとLLVMのimport libraryを混在させると、過去の環境ではRust起動時にDEP違反となった。コマンドがビルド成功しても実行できるか確認する。詳細は [validation.md](validation.md)。

## 実行・手動確認

```powershell
./target/release/living-wallpaper.exe --preview
./target/release/living-wallpaper.exe --preview --smoke-test
./target/release/living-wallpaper.exe --smoke-test
./target/release/living-wallpaper.exe
```

`CARGO_TARGET_DIR` を変更している場合は実行パスを合わせる。`--smoke-test` は約8秒で終了し、フレームが1枚も描画されなければ失敗する。実壁紙モードではデスクトップ背景を一時的に変えるため、作業環境に合わせて実行する。

手動の受入確認は [requirements.md](requirements.md) の末尾と [validation.md](validation.md) の未確認項目を使う。トレイの項目を変更した場合は、終了・再起動後の設定反映も確認する。アイコンやマウスは実操作、バッテリー・ロック・スリープは対象PCの状態切替で確認する。

マウスや電源状態の実機テストをしていない場合、コード上の分岐があることを根拠に合格としない。

## テストの範囲

- 設定の範囲制限と旧設定の引継ぎ、新設定の優先。
- マウス接近・逃避・OFFの力。
- 群泳の継続時の数値の有限性、範囲内保持、障害物への非侵入。
- WGSLの構文・意味検査。
- シミュレーション性能測定は通常のテストではignore。必要な場合のみ次を実行する。

```powershell
cargo test --release --locked simulation_benchmark -- --ignored --nocapture
```

Windowsでは通常29件成功・性能測定1件ignoreが現在の基準（魚・背景のWGSL検証、孤立個体の復帰・速度上限を含む）。テストを追加したら件数・記録を更新する。

## CIと配布

`.github/workflows/windows.yml` はWindows上でrustfmt、パス置換とEXE検査を行う `build.ps1`、配布Artifactsの保存を実行する。CIでのビルド成功とGUI・壁紙の動作確認は別。実行ファイルとライセンスは配布し、`AGENTS.md`、`docs/codex/`、ローカル設定やログはランタイム配布へ含めない。

`--remap-path-prefix` の文字列置換はベストエフォートであり、リンカーや生成コードが作ったパスをすべて除く保証はない。そのためビルド後のバイト検査も行う。PDBやCargoの中間ファイルは配布しない。参考: [Rustのパス置換](https://doc.rust-lang.org/rustc/remap-source-paths.html)、[Cargoの環境変数](https://doc.rust-lang.org/cargo/reference/environment-variables.html)。

手元の検証結果を [validation.md](validation.md) に日付、対象コミット、OS・GPU、ツールチェーン、実行コマンド、成功・失敗、実行していない項目とともに追記する。

## GitHub Releasesへの自動配布

`.github/workflows/release.yml` は、GitHubのReleaseを **Publish release** したときに起動する。正式版とプレリリースの両方が対象。下書き保存やタグのpushだけでは起動しない。

1. このワークフローを含むコミットを `main` にpushする。
2. GitHubのReleases → Draft a new releaseで、そのコミット以降を指すタグ（例: `v0.1.0`）を選ぶ／作成する。
3. Publish releaseを押し、Actionsの **Windows Release** の完了を待つ。
4. ReleaseのAssetsに `living-wallpaper-windows-x64.zip` が添付される。

Releaseのタグが指すコミットをWindows x64 MSVCで整形確認・テスト・Releaseビルドする。既存の `build.ps1` によるパス置換・残存検査を通し、`dist/` の内容のみをZIPにする。ZIP直下に `living-wallpaper.exe`、README、ライセンス類が入る。Rustやビルドツールは利用者側には不要。GPU・壁紙の実動作はCIでは確認しない。

認証にはGitHub標準の `GITHUB_TOKEN` とジョブの `contents: write` を使う。PAT等の追加Secretは不要。ビルドや検査に失敗した場合はZIPを添付せずActionsを失敗させる。公開直後からビルド完了まではAssetsに実行用ZIPがない。失敗後は同じActions実行を再実行でき、同名ZIPは置換する。ソースの修正が必要なら修正コミットで新しいタグ・Releaseを作る。

公開後のAsset追加を禁止するimmutable releases設定とは併用しない。その設定を使用する場合は、下書きへの添付後に公開する別フローが必要。GitHub標準のSource code ZIPはソース配布であり、この実行用ZIPとは異なる。署名・インストーラー・macOS/ARM64向け配布は対象外。

仕様参照: [Releaseイベント](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release)、[gh release upload](https://cli.github.com/manual/gh_release_upload)。


## リソースとREADMEのサンプル

アイコンはbuild.rsからassets/app.rcをコンパイルする。MSVCはWindows SDKのrc.exe、GNUはwindresを必要とする（WINDRESで指定可）。フォントとそのライセンス、ICOを取得済みのソースへ同梱し、実行時にダウンロードしない。

READMEのassets/demo.gifは現行のsimulation.rs・fish.wgsl・background.wgslを使ったオフスクリーン描画を縮小・減色したもの。デスクトップや個人設定を含めない。背景・魚の表現を変更したときに更新し、生成条件・検証結果をvalidation.mdへ追記する。実壁紙・モニター統合の受入確認には使わない。
