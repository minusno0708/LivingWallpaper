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

## 正式リリース後の開発フロー

mainは統合先とし、作業ブランチからPRを経由して変更を取り込む。文書変更や小さな修正も同じ手順とし、mainへの直接コミット・pushは行わない。

### 作業開始

未コミット変更と現在のブランチを先に確認する。未完了の作業がある場合は保存・継続方法を確認し、下記を機械的に実行して混在させない。

```powershell
git status --short --branch
git fetch origin
git switch main
git pull --ff-only
git switch -c feat/<topic>
```

ブランチ名はfeat/（機能）、fix/（不具合）、docs/（文書）、chore/（保守）を使い、目的が分かる短い名前にする。関連するPRが既にある場合はそのブランチを継続する。mainが分岐していたら強制更新で消さず、履歴を確認する。

### commit・pushとPR

1. 差分・検証結果・公開可能な作者名義を確認する。
2. 対象ファイルを明示してコミットし、現在の作業ブランチへpushする。docs/local、個人情報、EXE・ログ・キャッシュを含めない。
3. mainをbaseとするPRを作成する。既存PRがある場合は同じブランチへ追加し、説明を現在の内容に更新する。
4. PRには問題と変更後の動作、確認したテスト、未検証の範囲を簡潔に記載する。

```powershell
git push -u origin <work-branch>
```

「commit・push」は作業ブランチの送信とPR作成・更新として扱う。mainへの直接pushやマージを意味しない。Windows CIはpull_requestで実行される。実行環境の権限や設定によりCIが起動・完了しているかはPRごとに確認する。

### マージと次の作業

ユーザーの明示的なマージ指示があったときに、PRの差分・競合・必要なチェックを確認してマージする。失敗したチェックを無視した強制マージは行わない。マージ方式はリポジトリ設定と指示に従い、未合意の方式を固定しない。完了後に最新mainを取得し、次の作業用ブランチを切る。

ブランチ保護・必須レビューのGitHub設定はこの文書で自動的に有効になるものではない。設定変更は別の明示指示を受けて実施する。公開済みのリリースタグは作業ブランチの更新に合わせて移動しない。

## コミット情報

公開される作者名・メールアドレスは各開発環境で確認する。通常の連絡用メールを公開したくない場合は、自分のGitHub用noreplyアドレスなどをリポジトリ単位で設定する。

この公開用ソースの初期履歴は、個人の連絡先を含まないプロジェクト名義で作成する。過去の作業履歴、元の退避ブランチ、元の `.git` ディレクトリは公開用リポジトリへ持ち込まない。

## 管理対象

Rust・WGSL、Cargo.lock、ビルド設定、ライセンス、`AGENTS.md` と `docs/codex/` はGitで追跡する。実行ファイル、ツールチェーン、`target/`、`dist/`、個人設定、ログ、認証情報は除外する。

ソースZIPはGit履歴を含まない。開発の継続にはcloneを使用する。実行用配布物は `build.ps1` で生成し、検証済みの出力だけを配布する。
