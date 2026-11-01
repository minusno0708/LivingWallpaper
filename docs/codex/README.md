# Codex向け開発資料

別のマシン・別のチャットでも開発を引き継ぐための資料です。Gitで追跡するため、リポジトリをcloneすると一緒に取得できます。一般向けの利用説明はルートの [README.md](../../README.md) に置きます。

端末情報・個人情報・個人向けの補足は、隣の `docs/local/` に分離します。`docs/local/` はGitおよびソース・実行用配布物に含めません。ローカル資料がある環境では `docs/local/README.md` から最新の引き継ぎ・環境・補足方針を確認し、ない環境ではこの共通資料だけで開発を進めます。

## 読み方

ルートの [AGENTS.md](../../AGENTS.md) を入口に、要件と現在地を確認してから実コードを調べます。`docs/codex/` の通常のMarkdownファイルがすべて自動で読み込まれるという前提は置きません。

| 資料 | 内容 | 主な参照タイミング |
|---|---|---|
| [eyes-scene.md](eyes-scene.md) | Eyesの演出・状態遷移・初期値 | Eyesの追加開発 |
| [requirements.md](requirements.md) | 合意済み要件、対象範囲、受入条件 | 新規作業・仕様判断 |
| [architecture.md](architecture.md) | モジュール構成、処理の流れ、OS依存、拡張時の注意 | 実装・設計変更 |
| [status.md](status.md) | 実装済み・未実装、開発経緯、未解決事項 | 作業開始・終了 |
| [development.md](development.md) | 別PCの開発環境、テスト、手動確認、配布 | 環境構築・検証 |
| [git-setup.md](git-setup.md) | 正規リモート、clone、GitHub連携、履歴の扱い | 別PC・Git操作 |
| [validation.md](validation.md) | 実施済み検証と当時の環境、未確認項目 | 証跡の確認・検証記録の追加 |
| [publication.md](publication.md) | 公開するソース・履歴・配布物の確認 | 公開・再配布前 |
| [assets/preview.png](assets/preview.png) | 旧名称時点のプレビュー画像 | 過去の見た目の参照 |

## 更新方針

- 要件はユーザーの合意、実装状況は現在のコード、検証結果は実際に実行した結果を根拠にする。
- 要件の追加・変更は `requirements.md`、設計判断は `architecture.md`、現在地は `status.md` に反映する。
- 検証は `validation.md` に日付・OS・ツールチェーン・コマンド・結果・未実施項目を追記する。過去の記録を新しい結果で上書きしない。
- 秘密情報、認証トークン、個人の環境変数、マシン固有の必須パスを書かない。サンプルのパスにはプレースホルダーを使う。
- `AGENTS.md` を入口として簡潔に保ち、詳細な記録はここへ置く。`Agent.md` 等の重複した入口は作らない。
- ここは資料の整理場所でありアクセス制御ではない。Gitを読める人・ツールからも参照できる。
- ランタイムの `dist/` には含めない。ソース配布・Gitのcloneには含める。

Codexの標準ファイル名・読込範囲は [OpenAI公式のAGENTS.md説明](https://learn.chatgpt.com/docs/agent-configuration/agents-md) を参照。
