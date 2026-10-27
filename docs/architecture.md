# 実装の構成（軽量ヘキサゴナル）

v0.2.0 の実装は、ドメイン・アプリケーション・アダプターの 3 層に分ける。仕様は [v0.2.0 の仕様](v0.2.0_claude/spec.md) を正とし、この文書はコードの置き場所と層の境界を決める。

## 1. 層と依存の向き

```
adapters ──▶ app ──▶ domain
 (CLI・ファイル・描画・    (ユースケース・   (図面・参照・ライブラリの
  ライブラリの保存先)        ポート)          規則。純粋な計算)
```

| 層 | 置き場所 | 持つもの | 使ってよいもの |
| --- | --- | --- | --- |
| ドメイン | `src/domain/` | 図面 XML の検査・展開、`.drawio.png` の読み書き、参照文字列、ライブラリとエントリー、参照の展開・圧縮、フォント指定、診断（エラーコード） | 標準ライブラリ、XML・JSON・圧縮・ハッシュなどの計算用クレート |
| アプリケーション | `src/app/` | ユースケース（`read`・`write`・`expand`・`validate`・`library …`）と、外部に求める機能のポート（トレイト） | ドメイン |
| アダプター | `src/adapters/` | CLI（引数・出力・終了コード）、ファイル入出力、プロジェクト領域のライブラリ、組み込みライブラリ、レンダラー（Chromium・Desktop） | アプリケーション、ドメイン、OS・プロセス・ファイル・ネットワーク |

- 依存は外から内への一方向だけとする。`domain` は `app`・`adapters` を、`app` は `adapters` を参照しない。`tests/architecture.rs` がソースの `use` を調べてこれを検査する。
- `src/main.rs` は CLI アダプターを呼ぶだけとする。アダプターの組み立て（プロジェクトの探索、リポジトリ・レンダラーの選択）は、引数を解釈した後に CLI アダプターの `Context` が行い、ユースケースに渡す。
- ドメインの規則を含まないコマンド（`init`・`skill`・`licenses`）は、ユースケースを経ずに CLI アダプターが直接扱う。

## 2. ポート（`src/app/ports.rs`）

ユースケースが外部に求める機能は、アプリケーション層のトレイトとして定義し、アダプターが実装する。

| ポート | 役割 | 実装（アダプター） |
| --- | --- | --- |
| `LibraryRepository` | 1 つの名前空間のライブラリを名前で読む・一覧する（読み取り専用のリポジトリ） | `ProjectLibraries`（スコープなし）、`BuiltinLibraries`（`@drawio`） |
| `Renderer` | 図面 XML の 1 ページを PNG の画素にする | `ChromiumRenderer`（`vscode`・`desktop-compat`・`raw`）、`DesktopAppRenderer` |

ポートの境界では次を守る。

1. **引数と戻り値はドメインの型にする。** `LibraryName`・`Library`・XML の文字列・PNG のバイト列などを渡す。`Path`・`clap` の型・`roxmltree` のノード・`serde_json::Value` はポートを越えない。ファイルの場所や保存形式はアダプターの内側に閉じる。
2. **エラーはドメインの診断にする。** ポートは `Result<T, Diagnostic>` を返し、アダプターが自分の失敗を `RENDERER_NOT_FOUND`・`RENDER_FAILED`・`LIBRARY_INVALID` などのコードに対応付ける。アダプター内部では `anyhow` を使ってよい。
3. **リポジトリは所有権のあるドメインオブジェクトを返す。** 呼び出し側がリポジトリの内部状態に依存しないようにする。
4. **ポートは必要になった時点で足す。** ライブラリの保存・削除（`LibraryStore`）、設定（`config.toml`）・ロックファイル・取得元（`file:`・Git・HTTPS）・VS Code の設定は、該当するユースケースを実装するときにポートとして追加する。

## 3. ユースケース（`src/app/`）

- 入力はドメインの値（図面のバイト列、参照、オプションの構造体）で受け取り、結果は構造体で返す。標準出力への表示、終了コード、環境変数の読み取りはしない。表示形式（テキスト・`--json`）は CLI アダプターが決める。
- 1 件目で止めずに集めた問題は `Diagnostics`（複数の `Diagnostic`）として返す。
- 参照の解決は `Libraries`（`src/app/libraries.rs`）が行う。参照のスコープからリポジトリを選び（スコープなし → プロジェクト、`@drawio` → 組み込み）、エントリーをドメインの展開処理に渡す。展開処理自体はリポジトリを知らない。

## 4. ドメイン（`src/domain/`）

| モジュール | 内容 |
| --- | --- |
| `diagnostic` | エラーコード（仕様 2.3）、`Diagnostic`・`Diagnostics` |
| `codec` | DEFLATE の展開、XML の属性・テキストのエスケープ、数値の書式 |
| `element` | セルを組み立てるための小さな XML 要素の木 |
| `diagram` | 図面 XML の解析・圧縮ページの展開・構造の検査 |
| `drawio_png` | `.drawio.png` の PNG チャンクの検査、XML の取り出し・埋め込み |
| `reference` | `LibraryName`・`EntryId`・参照文字列・指紋付きの画像参照 |
| `library` | ライブラリ・エントリー、`<mxlibrary>` の読み込みと正規化した書式（仕様 9.3）、エントリー ID の導出 |
| `expand` | `libref`・`image=lib:` の展開 |
| `compact` | `read` の画像の参照への置き換え |
| `check` | 展開後の検査（ID・親子・接続先・残った参照） |
| `fonts` | フォント指定 |
| `preview` | `library preview` の一覧図面の配置 |

## 5. アダプター（`src/adapters/`）

| モジュール | 内容 |
| --- | --- |
| `cli` | `clap` の定義、ユースケースの呼び出し、テキスト・JSON の出力、診断の表示、終了コード |
| `fs` | 上限付きの読み込み、アトミックな書き込み |
| `project` | プロジェクト領域（`.drawio-png/`）の探索と、`ProjectLibraries`（`libraries/<名前>.xml`） |
| `builtin` | 同梱の図形カタログを読む `BuiltinLibraries` |
| `render` | Chromium・Desktop の起動と描画、同梱資材、縮小処理、シグナル処理 |

## 6. テスト

- ドメインはアダプターなしで単体テストする。
- ユースケースはインメモリのリポジトリ・レンダラーで検査する。
- `docs/v0.2.0_claude/examples/` の各ケースは受け入れテストに使う。「CLI に渡す XML」を展開した結果が「CLI がない場合の XML」と意味の上で一致することを確かめる。
- CLI は実行ファイルを起動して、入出力・終了コード・診断の形式を検査する。
