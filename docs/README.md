# ドキュメント

d-png-cli v0.2.0 の文書。仕様は [spec.md](spec.md) を正とし、他の文書と食い違う場合は spec.md に合わせる。

| 文書 | 内容 |
| --- | --- |
| [prd.md](prd.md) | 要件（目的・ユースケース・非機能要件） |
| [spec.md](spec.md) | CLI の仕様（共通の規約、コマンド、XML の参照、ライブラリ）と設計判断 |
| [usecase.md](usecase.md) | ユースケース A〜J。各ケースで「CLI に渡す XML」と「CLI がない場合の XML」を比較する |
| [vscode.md](vscode.md) | VS Code 拡張（`hediet.vscode-drawio` 1.9.0）との連携と、ライブラリ書式の制約 |
| [rendering.md](rendering.md) | レンダラーと描画互換性 |
| [fonts.md](fonts.md) | フォント指定 |
| [architecture.md](architecture.md) | 実装の構成（層・ポート・モジュール） |
| [examples/](examples/) | ユースケースの XML・ライブラリ・描画結果。受け入れテスト（`tests/acceptance.rs` など）が読む |
| [history/](history/) | v0.1.0 の開発時の検証記録と調査。当時の記録で、現在の仕様ではない |
