# Git運用

## 新しい開発環境

```powershell
git clone <repository-url> living-wallpaper
cd living-wallpaper
git remote -v
git status --short --branch
```

実際の取得先URLはリポジトリのClone欄を使用する。アカウント名や認証情報をソースへ固定しない。CodexのGitHub連携を使う場合も、対象リポジトリのアクセス権を別途確認する。

cloneしたルートの [AGENTS.md](../../AGENTS.md) を入口にする。環境構築は [development.md](development.md)、検証範囲は [validation.md](validation.md) を参照。

## 通常の更新

未コミット変更を確認したうえで、リモートとの差を調べる。

```powershell
git fetch origin
git log --oneline --left-right main...origin/main
git pull --ff-only
```

分岐やローカル変更がある場合は、内容を確認して統合する。既存の変更を消すための強制更新は行わない。送信はその作業でのユーザーの指示に従い、差分・送信先・認証情報や生成物の混入を確認する。

## コミット情報

公開される作者名・メールアドレスは各開発環境で確認する。通常の連絡用メールを公開したくない場合は、自分のGitHub用noreplyアドレスなどをリポジトリ単位で設定する。

この公開用ソースの初期履歴は、個人の連絡先を含まないプロジェクト名義で作成する。過去の作業履歴、元の退避ブランチ、元の `.git` ディレクトリは公開用リポジトリへ持ち込まない。

## 管理対象

Rust・WGSL、Cargo.lock、ビルド設定、ライセンス、`AGENTS.md` と `docs/codex/` はGitで追跡する。実行ファイル、ツールチェーン、`target/`、`dist/`、個人設定、ログ、認証情報は除外する。

ソースZIPはGit履歴を含まない。開発の継続にはcloneを使用する。実行用配布物は `build.ps1` で生成し、検証済みの出力だけを配布する。
