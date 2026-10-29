# プロダクト要件定義書 (PRD): `d-png-cli` v0.2.0

この文書は v0.2.0 の要件をまとめる。コマンド・XML・ライブラリの詳細な仕様は [v0.2.0 の仕様](spec.md)、VS Code 拡張との連携は [vscode.md](vscode.md)、作業例は [ユースケース](usecase.md) を正とする。v0.1.0 との互換性は持たない（変更点は [仕様の 12 章](spec.md#12-v010-からの変更)）。

## 1. 概要 (Overview)

`d-png-cli` は、VS Code の draw.io 拡張機能などで扱われる `.drawio.png` 形式のファイルを、AI エージェントと人間が相互に編集できるようにするための軽量かつ高速な CLI ツールである。

AI エージェントは図面全体を draw.io の XML で書き、標準入力で `d-png-cli write` に渡すと、検証・描画済みで XML を埋め込んだ `.drawio.png` が得られる。図形・アイコンは登録したライブラリの参照文字列で指定でき、長い style や Base64 の画像を書き写す必要がない。既存の `.drawio.png` からは編集用の XML を取り出し、編集後に書き戻せる。

---

## 2. 目的・背景 (Background & Goals)

* **課題**:
  1. `.drawio.png` 形式はバイナリ（PNG画像）とテキスト（XMLメタデータ）が混在しているため、プログラムや AI エージェントが直接読み書きを行うことが困難であった。
  2. ライブラリの部品（AWS の図形、Simple Icons のアイコン、社内のロゴなど）を XML に書くには、ライブラリの JSON・圧縮データを読み、style・画像・ID・座標を写して組み立てる必要があった。画像は 1 アイコンあたり数 KB の Base64 になり、エージェントのコンテキストを圧迫する。
  3. v0.1.0 の `insert` は 1 図形ずつコマンドで挿入して ID を受け取る流れで、図面全体を XML で書くエージェントの作業に合わなかった。
  4. 配布されている CDATA 形式などのライブラリは、draw.io では読めても VS Code 拡張 1.9.0 では読めない（[vscode.md 3 章](vscode.md#3-ライブラリファイルの書式の制約)）。人とエージェントが同じ部品を使えなかった。
* **目的**:
  1. XML を標準入力で渡すだけで、VS Code で開ける `.drawio.png` を作れるようにする。
  2. 部品を参照文字列（`dpng-ref`・`image=lib:`）で書けるようにし、ライブラリの中身を読まずに図面を作れるようにする。
  3. ライブラリを登録・取り込みし、同じファイルを VS Code 拡張からも使えるようにする。複数のリポジトリで使う部品は名前空間付きで配布し、版を固定する。
  4. CLI 実行時に XML の構文・構造・参照・ID・接続先を検証し、不正な入力による上書きを防ぐ。すべての描画・レイアウト不具合の防止は保証しない。
  5. 全コマンドで入出力・出力形式・終了コード・エラー形式を揃え、エージェントが迷わず使えるようにする。

---

## 3. ユースケース (Use Cases)

```
               read（登録画像 → image=lib: 参照）
[人間 (VS Code)] ──(GUIで作成・保存)──> filename.drawio.png ─────────────> 編集用 XML
        ▲                                     ▲                              │
        │                                     │                     [AI エージェント]
        │                                     │                     XML を編集・参照を追加
        │                                     │                              │
        └──(GUIで微調整)──────────────────────┴── write（展開・検証・描画・埋め込み） ◀┘
```

1. **AI による新規図面の生成**: エージェントが部品を `library search` で探し、参照文字列を含む XML を書いて `d-png-cli write` に標準入力で渡す。1 回の呼び出しで、VS Code で人が微調整できる `.drawio.png` ができる。
2. **AI による既存図面の更新**: 人が VS Code で作成・保存した `.drawio.png` を `read` で取り出す。登録済みの画像は指紋付きの参照に縮まるため、Base64 をコンテキストに入れずに編集でき、`write` で書き戻す。
3. **ライブラリの共有**: プロジェクト専用のライブラリを `library add` で登録し、社内で共有するライブラリをスコープ付き（`@acme/brand`）で `library install` する。`library vscode --write` で VS Code 拡張にも同じライブラリを設定する。
4. **共有ライブラリの配布と更新**: ライブラリを配布する側が `library pack` で取得元に版を作り、利用する各プロジェクトが `library update` で取り込む。
5. **CI/CD パイプラインでの図面検証**: PR 提出時などに `library install --frozen`・`validate`・`library sync --check` を通し、破壊された図面や VS Code 拡張で読めないライブラリの混入を防ぐ。

各ケースの手順と、CLI に渡す XML・CLI を使わない場合の XML の比較は [ユースケース](usecase.md) に示す。

---

## 4. CLI インターフェース仕様 (CLI Specification)

* **コマンド名**: `d-png-cli`
* **リポジトリ名**: `d-png-cli`

### 4.1. サブコマンド一覧

| コマンド | 内容 |
| --- | --- |
| `read [INPUT] [-o XML]` | 編集用の XML を出す。登録済みの画像を `image=lib:` の参照に縮める |
| `write [INPUT] -o PNG` | 参照を展開し、検証・描画して、全ページの XML を埋め込む |
| `expand [INPUT] [-o XML]` | 参照を展開した XML を出す。`write` が埋め込むものと同じ |
| `validate [INPUT]` | `write` の描画前までの処理（展開・検証）を行い、何も書かない |
| `init [DIR]` | プロジェクト領域を作る |
| `library list`・`search`・`show`・`preview` | ライブラリ・エントリーを調べる |
| `library add`・`remove`・`sync` | プロジェクト専用のライブラリを登録・削除・正規化する |
| `library install`・`update`・`pack` | スコープ付きのライブラリを取り込む・更新する・配布用の版を作る |
| `library vscode` | VS Code 拡張の設定を出力・更新する |
| `skill`・`licenses` | エージェント向けの手順書、ライセンスと通知 |

`insert`・`embed`・`extract` と、`--no-render`・`--no-validate`・`--chromium-mode`・`skill --full` は廃止する。

### 4.2. 共通の規約

* 図面の入力は位置引数 `INPUT` を 1 つ取り、省略または `-` で標準入力。XML と `.drawio.png` のどちらも受け付け、内容（PNG シグネチャ）で判別する。
* 出力先は `-o`, `--output`。XML・JSON は省略時に標準出力。PNG は必須で、`-o -` のときだけ標準出力。
* `--json` は `skill`・`licenses` を除く全コマンド共通で、標準出力に JSON 値を 1 つだけ出す。
* 入力 XML の問題は最初の 1 件で止めず、コード付き（`REF_NOT_FOUND`・`ID_DANGLING` など）でまとめて報告する。参照が見つからない場合は近い参照を候補に示す。
* 成功 `0`、入力・参照・描画・保存のエラー `1`、引数エラー `2`。診断・警告は stderr に出力する。
* 確認の問い合わせはしない。

詳細は [仕様の 2・3 章](spec.md#2-共通の規約)。

### 4.3. 参照

* 参照文字列は `<ライブラリ名>/<エントリー ID>`（プロジェクト専用）、`@<スコープ>/<ライブラリ名>/<エントリー ID>`（取り込み）。タイトルや番号では指定しない。
* セルの属性 `dpng-ref="<参照>"` はエントリーのセル群に展開し、プレースホルダーの位置・ラベル・style・サイズ・独自データなどで上書きできる。style の `image=lib:<参照>` は画像の値だけを置き換える。
* 展開後のセルに参照は残さない。PNG に保存するのは普通の draw.io XML で、開くのに CLI もライブラリも要らない。

詳細は [仕様の 4 章](spec.md#4-図面-xml-と参照)。

---

## 5. 自動バリデーション仕様 (Validation Rules)

`write`・`expand`・`validate` は、次の検証を行う。違反は全ページ分をまとめて報告し、終了コード `1` で既存の出力を変更しない。

```
[入力 XML/stdin/PNG]
       │
       ▼
1. XML Syntax Check (Well-formed)
       │
       ▼
2. Root Tag Check (<mxfile> / <mxGraphModel>)
       │
       ▼
3. Essential Cell Check (id="0" & id="1")
       │
       ▼
4. Reference Expansion (dpng-ref / image=lib:)
       │
       ▼
5. Expanded Model Check (ID・親子・接続先・残った参照)
       │
       ▼
 [描画・保存 (write)]
```

1. **XML 構文チェック**: タグの対構造、エスケープ漏れ、文字コードが正常な XML であるか。DTD・外部エンティティは拒否する。
2. **ルート要素チェック**: ルート要素が `<mxfile>` または `<mxGraphModel>` であるか。
3. **基盤セル存在チェック**: 各ページの `<mxGraphModel><root>` 直下に `id="0"` および `id="1"` の `<mxCell>` が存在するか。圧縮ページは展開して検証し、空の `<mxfile>` や復号不能なページは拒否する。
4. **参照の展開**: 参照先が登録・取り込み済みのライブラリにあるか、エントリーの種類と使い方・上書きが合っているか、展開で作る ID が既存の ID と重ならないか。取り込んだライブラリはロックファイルの完全性と照合する。
5. **展開後の検査**: ページ内の ID の一意性、`parent`・`source`・`target` の参照先が同じページにあること、`parent` が循環しないこと、参照が残っていないこと。

---

## 6. ライブラリ

* 参照できるのは、プロジェクトに登録・取り込みしたライブラリと、ユーザー領域のグローバルのライブラリだけとする。任意のパス・環境変数でライブラリを指す手段は設けない。プロジェクトのライブラリなら、同じ XML がどの環境でも同じ部品に展開される。
* CLI はライブラリを同梱しない。draw.io のサイドバーの図形（AWS・Azure など）を使う場合は、利用者が使う図形を draw.io でライブラリにして登録する。図形集・アイコンのライセンスと商標の条件は、登録する利用者が確認する。
* ライブラリ名には npm と同じ名前空間を持たせる。

| 種類 | 名前 | 追加の方法 |
| --- | --- | --- |
| プロジェクト専用 | `components` | `library add`。ファイルを Git にコミットする |
| 取り込み | `@acme/brand` | `library install`。取得元は `config.toml` の `[scopes]`、版は `lock.json` で固定する |

* どちらも `--global` でユーザー領域に置けば、プロジェクトごとに `init` や登録をせずに、すべてのプロジェクトで使える。プロジェクトに同じ名前のライブラリがあればプロジェクトのものを使う。グローバルのライブラリの参照を含む XML は、ほかの人の環境・CI では展開できない（保存した PNG を開くことには影響しない）。参照を含む XML を共有するリポジトリの部品は、プロジェクトに置く。

* 取得元はディレクトリ（`file:`）・Git・HTTPS とし、公開・認証を扱うレジストリーサーバーは作らない。ネットワークに接続するのは `library install`・`library update` だけとする。
* 登録・取り込みするファイルは、VS Code 拡張と draw.io の両方が読める書式（CDATA・実体参照を使わず、JSON の `\u003c` などで特殊文字を表す）とする。エントリーには安定した ID を付ける。
* `library vscode --write` は VS Code 拡張の `customLibraries` を生成する。

詳細は [仕様の 9 章](spec.md#9-ライブラリ) と [vscode.md](vscode.md)。

---

## 7. レンダリング戦略

`write --renderer` で描画方式を選ぶ。v0.1.0 の `--renderer` と `--chromium-mode` を 1 つにまとめ、既定を VS Code 拡張と同じ見た目になる `vscode` にする。Desktop の有無で結果が変わらないよう、自動の切り替え（v0.1.0 の `auto`）はしない。

| 値 | 描画 |
| --- | --- |
| `vscode`（既定） | Chromium。VS Code 拡張 1.9.0（draw.io 26.0.2）互換の SVG→Canvas。XML の `scale`・`border` を反映 |
| `desktop-compat` | Chromium。Desktop 互換（DPR 2 で取得し Hamming1 で縮小） |
| `raw` | Chromium。DPR 1 の取得結果を画素加工せず使う |
| `desktop-app` | draw.io Desktop 27.0.2 以降のアプリ |

* 優先順位は、CLI 指定 → `D_PNG_RENDERER` → プロジェクトの `config.toml` → ユーザーの `config.toml` → `vscode`。指定したレンダラーが見つからない場合や描画に失敗した場合はエラーにし、既存出力を維持する。
* 指定ページ（既定は先頭）を描画し、全ページの XML を埋め込む。
* Chromium の検索順は `D_PNG_CHROME_PATH` → `CHROME_PATH` → PATH → OS標準パス。PATH・OS標準パスでは Chrome を Chromium より優先する。`D_PNG_CHROME_ARGS` は POSIX 形式で分割し、シェルを起動しない。
* Chromium は基本描画資材をバイナリに同梱し、`D_PNG_DRAWIO_WEB_PATH` で `export3.html` を含むローカル Web 資材を指定できる。不正指定ではエラーとし、資材の自動取得はしない。同梱対象は基本図形・接続線・日本語・HTMLラベル・埋め込み画像。必要資材が欠ける図面や未知の図形ではエラーにする。
* 外部HTTP(S)画像・フォント等の取得は既定で禁止し、`--allow-network`（`desktop-app` 以外）で許可できる。
* Rust から CDP で専用プロファイルの Chromium を制御し、ループバックの資材サーバーを使用する。起動から PNG 取得まで60秒。タイムアウト・失敗時もブラウザーとサーバーを片付け、既存出力を維持する。取得画像・中間SVG／Canvas は RGBA で 64 MiB 以下に制限する。詳細は [描画互換性](rendering.md) に記載する。
* Desktop の検索順は `D_PNG_DRAWIO_PATH` → PATH の `drawio` / `draw.io` → OS 標準インストール先。明示指定が不正な場合はエラーとし、別の候補には切り替えない。`D_PNG_DRAWIO_ARGS` は全 OS 共通の POSIX シェル形式で分割し、シェル実行・展開は行わない。追加引数は `d-png-cli` が生成する描画引数の前に渡す。Linux のディスプレイがない環境では利用者が Xvfb 等を用意する。`d-png-cli` は sandbox を自動無効化しない。
* `write --metadata-only` はレンダラーを検索・起動せず、ベース画像または新規透明 PNG に XML を保存する。画像と XML が同期されない旨を警告する。
* `write --default-font FAMILY` と繰り返し指定可能な `--fallback-font FAMILY`（または `config.toml` の `[render]`）により、レンダラーへ渡すセルのフォント候補を統一できる。既存の明示指定を優先し、設定を全ページの埋め込み XML にも保存する。適用範囲と互換性は [フォント指定の仕様](fonts.md) に従う。
* 資材のライセンス・出典・ハッシュを記録する。`d-png-cli licenses` で同梱資材の表示・全文を取得可能とする。

---

## 8. 非機能要件 & 技術スタック (Non-functional Requirements)

### 8.1. 非機能要件

* **高速起動**: AI エージェントのツール呼び出しループを阻害しないため、リリースビルドの起動と小規模入力の処理を数十ミリ秒以下の目標で測定する。外部レンダラーの起動・描画時間は別途評価する。
* **ポータビリティ**: 外部依存（Node.js や Python ランタイム等）を持たない単一バイナリ（Single Binary）として配布可能であること。外部コマンドは、Git の取得元を使う場合の `git` と、HTTPS の取得元を使う場合の `curl` だけを使う。
* **アトミック書き込み**: 出力先と同じディレクトリの一時ファイルに保存・同期後、アトミックに置換する。検証・描画・保存失敗時は既存出力を維持する。ライブラリの登録・取り込みも、失敗時はファイル・設定・ロックファイルを変更しない。
* **再現性**: 参照の解決はプロジェクト内のファイルと CLI の版だけで決まる。ネットワークに接続するのは `library install`・`library update` だけで、描画・検証では接続しない（`--allow-network` で外部の画像・フォントを許可した描画を除く）。

### 8.2. 技術スタック (Rust)

* **言語**: Rust
* **使用クレート**:
  * `clap`: CLI 引数・サブコマンドの構文解析
  * `png`: PNG 画像の検査と透明画像の生成。メタデータはチャンク単位で直接置換し、`crc32fast` で CRC を検証・生成する
  * `flate2`: draw.io データの Deflate 圧縮・解凍
  * `base64`, `urlencoding`: メタデータのエンコード・デコード処理
  * `roxmltree`: XML の検証とソース範囲に基づくページ展開。全体の再シリアライズを避け、未知要素や属性を保持する
  * `serde`, `serde_json`: ライブラリの JSON、`versions.json`・`lock.json`、`--json` の出力
  * `toml_edit`: `config.toml` の読み込みと、コメント・順序を保った `[dependencies]` の書き換え
  * `sha2`: 完全性（`sha256-…`）と画像の指紋
  * `unicode-normalization`: エントリー ID を導くときの NFKC 正規化
  * `tempfile`: 同一ディレクトリでのアトミック保存と描画用一時ファイル
  * `shell-words`: `D_PNG_CHROME_ARGS`・`D_PNG_DRAWIO_ARGS` の引用符・エスケープを含む引数分割
  * `tungstenite`, `tiny_http`: Chromium の制御（CDP）と、ループバックの描画資材サーバー
  * `anyhow`: アダプター内部のエラー処理
* HTTPS の取得元は `curl` コマンド、Git の取得元は `git` コマンドで取得し、HTTP クライアントのクレートは使わない。

### 8.3. 入出力の制約

* UTF-8 XML（BOM 可）を扱い、DTD・外部エンティティは拒否する。
* 入力・展開データ・出力 PNG・デコード後の画像バッファはそれぞれ 64 MiB を上限とする。
* PNG シグネチャ、チャンク境界、CRC、画像データを検査する。
* `tEXt` / `zTXt` の `mxfile` / `mxGraphModel` を読み取る。旧 raw DEFLATE と二重 URL エンコードにも対応する。
* 競合する図面メタデータはエラー。同一内容の重複は許容する。書き込み時は図面メタデータを一つに統一する。

## 9. 将来の拡張性 (Future Scope)

* **GitHub Actions 対応**: リポジトリ内の `.drawio.png` が常に正しい状態であることを自動チェックする CI アクションの提供。
* **Diff サブコマンド (`d-png-cli diff`)**: 2つの `.drawio.png` の XML 構造差分（追加・削除された図形やテキスト）をテキストベースで出力する機能。
* **既存図面のセルの登録**: 図面のセルをライブラリのエントリーとして登録する機能（`library add diagram.drawio --cell ID`）。
* **レジストリーサーバー**: 公開・認証・版の取り下げを CLI で扱う配布の仕組み。

v0.2.0 の仕様で決めていない事項は [仕様の 13 章](spec.md#13-未決定事項) にまとめる。
