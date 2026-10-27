# drawio-png-cli v0.2.0 仕様

この文書は v0.2.0 の CLI 仕様である。v0.1.0 との互換性は考慮しない。要件は [prd.md](prd.md)、設計判断の理由は [1.1](#11-主な設計判断)、VS Code 拡張との連携は [vscode.md](vscode.md)、作業例と XML の比較は [usecase.md](usecase.md) を参照。

## 1. 設計方針

1. **XML を標準入力で渡すと PNG ができる。** エージェントは図面全体の draw.io XML を書き、`drawio-png-cli write -o out.drawio.png` に渡す。1 図形ずつ挿入する `insert` は廃止する。
2. **ライブラリは登録し、参照文字列で使う。** 図形・アイコンは `<ライブラリ>/<エントリー ID>` で指定し、長い style や Base64 を書き写さない。参照できるのは登録・取り込み済みのライブラリだけとする。draw.io のサイドバーの図形も同梱せず、使う図形は利用者がライブラリにして登録する。
3. **ライブラリ名に名前空間を持たせる。** npm のスコープと同じく、共有するライブラリは `@acme/brand` のようにスコープを付け、スコープごとに取得元（ディレクトリ・Git・HTTPS）を設定して `library install` で取り込む。版はロックファイルで固定する。スコープのない名前はそのプロジェクト専用のライブラリとする。
4. **登録したライブラリは VS Code 拡張でもそのまま使える。** 登録先は、VS Code 拡張と draw.io の両方が読めるライブラリファイルとし、拡張の設定も CLI が生成する。
5. **保存される図面は普通の draw.io XML。** PNG に埋め込む XML には参照を残さない。VS Code・draw.io で開くときに CLI もライブラリも不要。
6. **操作は一貫させる。** すべてのコマンドが同じ入出力・出力形式・終了コード・エラー形式に従う（[2 章](#2-共通の規約)）。

### 1.1 主な設計判断

1. **`insert` を廃止し、XML に参照を書く。** エージェントは図面全体を XML で書くため、1 図形ずつコマンドで挿入して ID を受け取る流れは合わない。部品は `libref="<ライブラリ>/<ID>"` のプレースホルダーとして書き、`write` が展開する。1 回の呼び出しで図面ができ、中間ファイルも要らない。
2. **参照は 1 つの文字列にする。** 登録時にエントリーへ安定した ID を付けるため、タイトルの重複・空・`/` を含むタイトル・番号のずれを考えずに済む。PRD 4.2 で計画していた「ライブラリ名・タイトル・番号」と「画像ハッシュ」の 2 系統の指定は、同じ参照文字列の 2 つの書き方（セル群の `libref` と画像だけの `image=lib:`）にまとめた。
3. **ライブラリの登録を必須にし、ライブラリを同梱しない。** 参照はプロジェクトに登録・取り込みしたライブラリだけで解決する。draw.io のサイドバーの図形（AWS・Azure など）も同梱せず、使う図形は利用者が draw.io でライブラリにして登録する。図形集・アイコンにはそれぞれのライセンス・商標の条件があり、何を使うかは利用者が決めるためである。環境変数で任意のパスを指すのをやめ、どこで実行しても（CI・他の開発者の環境でも）同じ部品になるようにした。登録したファイルはそのまま VS Code 拡張の設定から読ませる。
4. **ライブラリ名に npm と同じ名前空間を入れる。** スコープのない名前（`components`）はそのプロジェクト専用、スコープ付きの名前（`@acme/brand`）は取得元から取り込む共有ライブラリとする。スコープごとに取得元（ディレクトリ・Git・HTTPS）を設定し、`library install` で取り込んで版を `lock.json` に固定する。同じ名前の部品集が社内・個人にあっても参照が衝突せず、版を上げるまで部品が変わらない。公開・認証を扱うレジストリーサーバーは作らず、Git などの既存の手段で配布する（[9.5](#95-スコープと取得元)）。
5. **登録ファイルは VS Code 拡張が読める書式で書く。** 拡張 1.9.0 は `<mxlibrary>` の中身を実体参照を解かずに `JSON.parse` するため、CDATA や `&lt;` を使ったライブラリは draw.io で読めても拡張では読めない。CLI は XML の特殊文字を JSON 側のエスケープ（`\u003c` など）で書き、draw.io と拡張の両方で同じ内容になるようにする（[vscode.md](vscode.md#3-ライブラリファイルの書式の制約)）。
6. **保存する XML には参照を残さない。** PNG の中は普通の draw.io XML なので、VS Code・draw.io で開くのに CLI もライブラリも要らない。
7. **`read` と `expand` を逆向きの対にする。** `read` は登録済みの画像を指紋付きの `image=lib:` に縮めてエージェントのコンテキストを節約し、`expand` は参照を展開した単体の XML を出す。v0.1.0 の `--keep-images` のようなオプションは要らない。
8. **既定のレンダラーを VS Code 互換にする。** `--renderer` と `--chromium-mode` を 1 つにまとめ、既定を `vscode` にした。draw.io Desktop が入っているかどうかで結果が変わらない。
9. **PNG を読む・書くコマンドを `read`・`write` と呼ぶ。** ツール名と合わせて「drawio-png を読む・書く」と読め、標準出力・標準入力との向きも名前どおりになる。描画しない `write --metadata-only` でも名前と矛盾しない。`write` が描画を伴うことはヘルプと手順書で明記する（[3 章](#3-コマンド一覧)）。
10. **入出力・出力形式・エラーを全コマンドで揃える。** 入力は位置引数か標準入力、出力は `-o`、機械向けは `--json`、エラーはコード付きで全件をまとめて報告する（[2 章](#2-共通の規約)）。

## 2. 共通の規約

### 2.1 コマンドの形と入出力

```
drawio-png-cli <コマンド> [INPUT] [オプション]
drawio-png-cli library <操作> [引数] [オプション]
```

| 規約 | 内容 |
| --- | --- |
| 図面の入力 | 位置引数 `INPUT` を 1 つ取る。省略または `-` で標準入力。XML（`<mxfile>`・`<mxGraphModel>`）と `.drawio.png` のどちらも受け付け、拡張子ではなく内容（PNG シグネチャ）で判別する |
| 出力先 | `-o`, `--output`。テキスト（XML・JSON・表）は省略時に標準出力。PNG は必須で、`-o -` を明示した場合だけ標準出力に書く |
| 上書き | 出力先は入力と同じパスでもよい。同じディレクトリの一時ファイルに書き、同期後にアトミックに置き換える。失敗時は既存の出力を変更しない。出力先ディレクトリは作成しない |
| 機械向け出力 | `--json` は全コマンド共通で、標準出力に JSON 値を 1 つだけ出す。主出力（XML）を標準出力に出すコマンドで `--json` を使う場合は `-o` を必須にする。標準出力には主出力か JSON のどちらか 1 つしか出さない |
| 診断 | 警告・エラーは標準エラー。`--json` 指定時は [2.3](#23-エラーと警告) の JSON |
| 終了コード | `0` 成功、`1` 入力・参照・描画・保存のエラー、`2` 引数のエラー |
| 参照文字列 | `[@<スコープ>/]<ライブラリ名>/<エントリー ID>`。XML の中でもコマンドの引数でも同じ書式（[4.2](#42-参照文字列)） |
| 対話 | 確認の問い合わせはしない。端末でない出力には色や進捗表示を付けない |

`--library NAME`・`--scope SCOPE` は検索・一覧の絞り込みだけに使う。XML の参照は常に完全な参照文字列で書くため、検索時の指定で参照先が変わることはない。`--scope` は名前空間（`acme`・`drawio` など）を指し、ライブラリを置く領域（2.2）を指すことはない。

### 2.2 プロジェクト領域とユーザー領域

ライブラリはプロジェクト領域だけに置き、ユーザー領域には個人の設定だけを置く。

| 領域 | 場所 | 置くもの |
| --- | --- | --- |
| プロジェクト | カレントディレクトリから親へ向かって最初に見つかる `.drawio-png/`。`--project DIR` または `DRAWIO_PNG_PROJECT` で指定できる | ライブラリ、依存関係、ロックファイル、描画の設定。Git で共有する。VS Code では `${workspaceFolder}` から参照する |
| ユーザー | `DRAWIO_PNG_HOME`。未設定なら Linux は `$XDG_CONFIG_HOME/drawio-png-cli`（未設定なら `~/.config/drawio-png-cli`）、macOS は `~/Library/Application Support/drawio-png-cli`、Windows は `%APPDATA%\drawio-png-cli` | `config.toml` だけ（描画の既定値、個人・社内のスコープの取得元） |

```
.drawio-png/
  config.toml                # 描画の既定値・スコープの取得元・依存ライブラリ（2.4）
  lock.json                  # 取り込んだライブラリの版と完全性（9.6）
  .gitignore                 # 取り込んだライブラリ（libraries/@*/）を Git から除外する
  libraries/
    <名前>.xml               # library add で登録したプロジェクト専用のライブラリ（9.3 の書式）
    @<スコープ>/<名前>.xml    # library install で取り込んだライブラリ（9.3 の書式）
```

ユーザー領域にライブラリを置かないのは、同じ XML がどの環境でも同じ部品に展開されるようにするためである。個人のライブラリも、`file:` の取得元（[9.5](#95-スコープと取得元)）から各プロジェクトに取り込む。

検索の高速化のための索引と、取得元から取得したファイルは、プロジェクト内には置かず、OS のキャッシュディレクトリに置く（索引はライブラリファイルのパス・サイズ・更新時刻・ハッシュを、取得したファイルは完全性をキーにする）。

### 2.3 エラーと警告

入力 XML の問題は、最初の 1 件で止めずにまとめて報告する（最大 100 件）。各項目にコードを付け、エージェントがコードで対処を選べるようにする。

テキスト表示（標準エラー）:

```
error[REF_NOT_FOUND] page 1 "構成図", cell "db": shapes/cylnder is not found
  candidates: shapes/cylinder
```

`--json` 指定時（標準エラーに 1 行）:

```json
{"errors":[{"candidates":["shapes/cylinder"],"cell":"db","code":"REF_NOT_FOUND","message":"shapes/cylnder is not found","page":1,"pageName":"構成図","ref":"shapes/cylnder"}],"warnings":[]}
```

| コード | 内容 |
| --- | --- |
| `USAGE` | 引数の誤り（終了コード `2`） |
| `INPUT_READ`・`LIMIT_EXCEEDED` | 入力を読めない、サイズの上限（64 MiB）を超えた |
| `PNG_INVALID`・`PNG_NO_DIAGRAM` | PNG が壊れている、図面のメタデータがない・競合する |
| `XML_SYNTAX`・`XML_STRUCTURE` | XML の構文、ルート要素・ページ・基盤セル（`0`・`1`）の誤り |
| `ID_DUPLICATE`・`ID_DANGLING` | ページ内の ID の重複、存在しない `parent`・`source`・`target` |
| `REF_SYNTAX`・`REF_NOT_FOUND` | 参照文字列の形式の誤り、参照先がない（`candidates` に近い参照を最大 5 件） |
| `REF_KIND` | エントリーの種類と使い方が合わない（画像でないエントリーへの `image=lib:` など） |
| `REF_OVERRIDE` | 許されない上書き（グループへの幅・高さの指定など） |
| `REF_ID_CONFLICT` | 展開で作る ID が既存の ID と重なる、消える ID を参照している |
| `IMAGE_CHANGED` | 指紋付きの画像参照に一致する画像が登録ライブラリにない |
| `LIBRARY_EXISTS`・`LIBRARY_INVALID` | 同名のライブラリがある、ライブラリ・エントリーを解析できない |
| `PROJECT_NOT_FOUND` | プロジェクト領域が必要な操作（登録・取り込み・取り込んだライブラリの参照）で、`.drawio-png/` が見つからない |
| `SCOPE_UNKNOWN`・`SOURCE_FETCH` | スコープに取得元が設定されていない、取得元から取得できない |
| `VERSION_NOT_FOUND`・`VERSION_EXISTS` | 範囲に合う版がない、`library pack` で同じ版が既にある |
| `LIBRARY_NOT_INSTALLED`・`LIBRARY_INTEGRITY` | 依存にあるライブラリが取り込まれていない、取り込んだファイルがロックファイルの完全性と合わない |
| `LIBRARY_MANAGED` | 取り込んだライブラリを `library add`・`library remove` のエントリー指定などで直接変えようとした |
| `LOCK_OUTDATED` | `library install --frozen` で、依存関係とロックファイルが合わない |
| `RENDERER_NOT_FOUND`・`RENDER_FAILED`・`RENDER_TIMEOUT` | レンダラーがない・描画に失敗した・60 秒を超えた |
| `ASSET_MISSING`・`NETWORK_BLOCKED` | 描画資材が足りない、外部の画像・フォントの取得が許可されていない |
| `OUTPUT_WRITE` | 出力を書けない |

警告も同じ形式で `warnings` に入れる。警告だけなら終了コードは `0`。

### 2.4 設定ファイル

`config.toml` には、描画の既定値と、ライブラリの取得元・依存関係を書く。プロジェクトで描画方式・フォント・ライブラリの版を揃え、誰が・どのエージェントが実行しても同じ PNG にするためのもの。

```toml
[render]
renderer = "vscode"
default-font = "Noto Sans CJK JP"
fallback-fonts = ["Noto Sans CJK JP", "Noto Color Emoji"]
allow-network = false

[scopes]
acme = "git+https://github.com/acme/drawio-libraries.git"
partner = { url = "https://libs.example.com/drawio/", token-env = "PARTNER_LIBS_TOKEN" }

[dependencies]
"@acme/brand" = "^1.0.0"
```

| セクション | 書ける場所 | 内容 |
| --- | --- | --- |
| `[render]` | プロジェクト・ユーザー | 描画の設定。`write` と `library preview` が使う。フォントの指定は、埋め込む XML を同じにするため `expand` にも適用する |
| `[scopes]` | プロジェクト・ユーザー | スコープと取得元の対応（[9.5](#95-スコープと取得元)）。同じスコープはプロジェクトの設定を使う |
| `[dependencies]` | プロジェクトだけ | 取り込むライブラリと版の範囲（[9.6](#96-依存関係とロックファイル)）。`library install`・`update`・`remove` が書き換える。手で書いてもよい |

優先順位は、コマンドのオプション → 環境変数 → プロジェクトの `config.toml` → ユーザーの `config.toml` → 既定値。未知のキーはエラーにする。CLI が `config.toml` を書き換えるときは、コメント・キーの順序・他のセクションを保持する。トークンなどの秘密の値は書かない（`token-env` には環境変数の名前を書く）。

### 2.5 環境変数

| 環境変数 | 内容 |
| --- | --- |
| `DRAWIO_PNG_PROJECT` | プロジェクト領域を含むディレクトリ |
| `DRAWIO_PNG_HOME` | ユーザー領域（個人の `config.toml` を置くディレクトリ） |
| `[scopes]` の `token-env` で指定した名前 | HTTPS の取得元に送るトークン（[9.5](#95-スコープと取得元)） |
| `DRAWIO_PNG_CACHE` | 取得元から取得したファイルのキャッシュ。未設定なら OS のキャッシュディレクトリ（Linux は `$XDG_CACHE_HOME/drawio-png-cli`、macOS は `~/Library/Caches/drawio-png-cli`、Windows は `%LOCALAPPDATA%\drawio-png-cli`） |
| `DRAWIO_PNG_RENDERER` | 既定のレンダラー（[5.2](#52-レンダラー)） |
| `DRAWIO_PNG_CHROME_PATH`・`CHROME_PATH`・`DRAWIO_PNG_CHROME_ARGS` | Chrome / Chromium の実行ファイルと追加引数（v0.1.0 と同じ） |
| `DRAWIO_PNG_DRAWIO_PATH`・`DRAWIO_PNG_DRAWIO_ARGS` | draw.io Desktop の実行ファイルと追加引数（v0.1.0 と同じ） |
| `DRAWIO_PNG_DRAWIO_WEB_PATH` | 追加の draw.io Web 資材（v0.1.0 と同じ） |

`DRAWIO_PNG_LIBRARY_PATH`・`DRAWIO_PNG_CHROMIUM_MODE` は廃止する。

## 3. コマンド一覧

| コマンド | 入力 → 出力 | 内容 |
| --- | --- | --- |
| `read [INPUT] [-o XML]` | 図面 → XML | 編集用の XML を出す。登録済みの画像を `image=lib:` の参照に縮める |
| `write [INPUT] -o PNG` | 図面 → `.drawio.png` | 参照を展開し、検証・描画して、全ページの XML を埋め込む |
| `expand [INPUT] [-o XML]` | 図面 → XML | 参照を展開した XML を出す。`write` が埋め込むものと同じ |
| `validate [INPUT]` | 図面 → 検査結果 | `write` の描画前までの処理（展開・検証）を行い、何も書かない |
| `init [DIR]` | — | プロジェクト領域を作る |
| `library list` | — | ライブラリの一覧 |
| `library search [QUERY...]` | — | エントリーの検索 |
| `library show REF` | — | ライブラリ・エントリーの詳細（style・セル構成・配置例） |
| `library preview QUERY... -o PNG` | — | 該当するエントリーを並べた PNG |
| `library add SOURCE...` | — | ライブラリ・画像ファイルを、プロジェクト専用のライブラリ（スコープなし）として登録する |
| `library install [NAME[@範囲]...]` | — | スコープ付きのライブラリを取得元から取り込む |
| `library update [NAME...]` | — | 取り込んだライブラリを新しい版にする |
| `library pack SOURCE... --to DIR` | — | 取得元に置くライブラリの版を作る |
| `library remove REF` | — | ライブラリ・エントリーの登録、取り込みを消す |
| `library sync [NAME...]` | — | 人が編集したライブラリファイルを正規化する |
| `library vscode` | — | VS Code 拡張の設定を出力・更新する |
| `skill` | — | エージェント向けの手順書 |
| `licenses` | — | ライセンスと通知 |

図面を受け取る 4 コマンドの関係:

```
              read（登録画像 → image=lib: 参照）
.drawio.png ─────────────────────────────────▶ 編集用 XML ──(エージェントが編集・参照を追加)──┐
   ▲   │                                                                                  │
   │   └── expand（参照を展開した単体の XML）                                               │
   └──────── write（expand → 検証 → 描画 → 埋め込み） ◀─────────────────────────────────────┘
                validate（write の描画前までを実行し、何も書かない）
```

- `read`・`write` は、ツール名の対象である `.drawio.png`（drawio-png）を読む・書く動詞とした。`drawio-png-cli read` は PNG を読んで XML を標準出力へ、`drawio-png-cli write` は標準入力の XML から PNG を書く。
- `write` は名前に描画を含まないが、既定で XML を検証・描画してから書く。描画には Chrome / Chromium（`desktop-app` では draw.io Desktop）が必要で、数秒かかる。ヘルプと `skill` の手順書ではこの点を明記する。
- `read` と `expand` は逆向きの処理で、どちらも XML と PNG の両方を受け付ける。

## 4. 図面 XML と参照

### 4.1 受け付ける XML

- UTF-8（BOM 可）。ルートは `<mxfile>`（1 つ以上の `<diagram>`）または `<mxGraphModel>`。圧縮されたページは展開して処理する。
- 各ページの `<root>` に基盤セル `<mxCell id="0"/>` と `<mxCell id="1" parent="0"/>` が必要。省略時に補うことはしない。
- DTD・外部エンティティは受け付けない。入力・展開後のデータはそれぞれ 64 MiB まで。
- 参照を含まない普通の draw.io XML はそのまま受け付ける。参照は必要な箇所だけに使う。

### 4.2 参照文字列

```
<ライブラリ名>/<エントリー ID>
@<スコープ>/<ライブラリ名>/<エントリー ID>
```

| 例 | 意味 |
| --- | --- |
| `simple-icons/postgresql` | プロジェクトに登録したライブラリ `simple-icons` のエントリー `postgresql` |
| `@acme/brand/logo` | スコープ `acme` の取得元から取り込んだライブラリ `@acme/brand` のエントリー `logo` |
| `shapes/cylinder` | プロジェクトに登録したライブラリ `shapes`（draw.io の図形を利用者がまとめたもの）のエントリー `cylinder` |

- `@` で始まれば `@<スコープ>/<ライブラリ名>/<エントリー ID>` の 3 つ、そうでなければ `<ライブラリ名>/<エントリー ID>` の 2 つに `/` で分ける。数が合わなければ `REF_SYNTAX`。
- 版は書かない。取り込んだライブラリのどの版を使うかは、プロジェクトのロックファイルで決まる（[9.6](#96-依存関係とロックファイル)）。
- 大文字小文字を含め完全一致で解決する。タイトルや番号での指定、あいまいな一致はしない。見つからなければ `REF_NOT_FOUND` とし、近い参照を候補に示す。スコープは補わない（`brand/logo` を `@acme/brand/logo` に読み替えない。候補には示す）。
- 名前と ID の規則は [9.2](#92-名前とエントリー-id)。

参照の書き方は 2 つある。

| 書き方 | 展開されるもの | 使う場面 |
| --- | --- | --- |
| セルの属性 `libref="<参照>"` | エントリーのセル群（style・ラベル・独自データ・子セル・接続線）。指定した値で上書きできる | 図形・アイコン・グループ・接続線を追加する |
| style の `image=lib:<参照>` | `image` の値（実画像）だけ | 画像だけを使い、style・サイズを自分で決める。`read` が縮めた画像を戻す |

`libref` 属性と `lib:` で始まる `image` の値は予約する。利用者のデータに `libref` という名前の属性は使えない。

### 4.3 セル群の参照（`libref`）

`libref` を持つセル（プレースホルダー）を、エントリーのセル群に置き換える。

```xml
<mxCell id="orders" value="注文 DB" parent="1" libref="shapes/cylinder">
  <mxGeometry x="40" y="40" as="geometry"/>
</mxCell>
```

展開後（[usecase.md の A](usecase.md#3-a-drawio-の図形を登録したライブラリから追加する)）:

```xml
<mxCell id="orders" value="注文 DB" style="shape=cylinder3;whiteSpace=wrap;html=1;boundedLbl=1;backgroundOutline=1;size=15;" vertex="1" parent="1">
  <mxGeometry x="40" y="40" width="60" height="80" as="geometry"/>
</mxCell>
```

エントリーの種類（[9.4](#94-エントリーの種類)）によって、プレースホルダーに書ける値が決まる。

| 項目 | 規則 |
| --- | --- |
| 置き場所 | `<mxCell>` か、それを包む `<object>`・`<UserObject>` のどちらか 1 か所 |
| 位置 | エントリーのセル群の外接矩形の左上を、プレースホルダーの `mxGeometry` の `x`・`y`（省略時は `0`）に合わせる。`parent` はプレースホルダーのものを使う |
| ID | 最上位のセルが 1 つなら、そのセルにプレースホルダーの ID を付け、子孫のセルは文書順に `<ID>-1`、`<ID>-2`… とする。最上位のセルが複数（種類 `set`）なら、すべてのセルを `<ID>-1`、`<ID>-2`… とし、プレースホルダーの ID は残らない |
| セルの種類 | `vertex`・`edge` は省略できる。書いた場合はエントリーと一致しなければ `REF_KIND`。`set` では書かない |
| ラベル | `value`（ラッパーがあれば `label`）で最上位のセルのラベルを上書きする。省略時はエントリーのラベル |
| style | 最上位のセルの style にキー単位で重ねる（同じキーはプレースホルダーを優先）。名前付きスタイル（`=` のないトークン）は `REF_OVERRIDE` |
| 幅・高さ | 種類が `image`・`shape` のときだけ指定できる。片方だけなら縦横比を保つ |
| 独自データ | ラッパーの属性（`id`・`label`・`libref` 以外）をエントリーの属性に重ねる。同名はプレースホルダーを優先し、空文字も上書きとして扱う。ラッパーの要素名はプレースホルダーのものを使い、なければエントリーのものを使う。入れ子にはしない |
| その他の `mxCell` 属性 | `connectable`・`visible`・`collapsed` などは最上位のセルに上書きする |
| 接続線 | 種類 `edge` では `source`・`target` を書ける。両方あれば、エントリーの端点・経由点を捨て、プレースホルダーの `mxGeometry`（`relative="1"`、経由点）を使う。片方だけなら、接続しない側の端点はエントリーの点を `x`・`y` に合わせて移動して使う。どちらもなければエントリーの線を `x`・`y` に合わせて移動する |
| 参照の除去 | 展開したセルに `libref` は残らない。以後は普通のセルで、ライブラリの更新は反映されない |

- ラベル・style・その他の属性・独自データの上書きは、最上位のセルが 1 つのとき（`set` 以外）だけ使える。`set` でこれらを指定すると `REF_OVERRIDE`。
- 展開で作る ID がページ内の既存の ID と重なる場合、`set` のプレースホルダーの ID を他のセルが `parent`・`source`・`target`・`sourcePort`・`targetPort` で参照している場合は `REF_ID_CONFLICT`。
- 子のセルが何番になるかは `library show REF --json` の `cells` で事前に分かる。エージェントは展開を待たずに `<ID>-2` などへ接続線を書ける。
- エントリー内の `parent`・`source`・`target`・`sourcePort`・`targetPort` は振り直した ID に対応付ける。

### 4.4 画像の参照（`image=lib:`）

style の `image` の値に `lib:<参照>` を書くと、その値だけをエントリーの実画像（`data:` URI）に置き換える。ラベル・サイズ・他の style は書いたとおりに残る。

```xml
<mxCell id="logo" value="会社ロゴ" vertex="1" parent="1"
        style="shape=image;image=lib:@acme/brand/logo;aspect=fixed;verticalLabelPosition=bottom;verticalAlign=top;">
  <mxGeometry x="40" y="120" width="120" height="60" as="geometry"/>
</mxCell>
```

- 参照できるのは種類 `image` のエントリーだけ。それ以外は `REF_KIND`。
- 画像の値は、`xml` のエントリーではセルの style の `image` の値そのもの、`data` のエントリーでは draw.io がサイドバーから配置するときと同じ変換（`convertDataUri`: 最初の `;` から次の `,` の手前までを削る。例 `data:image/png;base64,AAA` → `data:image/png,AAA`）をした値とする。VS Code 拡張のライブラリ欄から人が置いた画像と同じ値になる。
- `lib:<参照>@<指紋>` の形は `read` が書く（[7 章](#7-read)）。指紋は画像の値（UTF-8）の SHA-256 の先頭 12 桁（小文字の 16 進数）。スコープ付きの参照でも指紋は最後の `@` の後に付く（例 `lib:@acme/brand/logo@37483e4b7f30`）。エントリー ID は `@` を含まないため、先頭の `@`（スコープ）と区別できる。指紋付きの参照は、プロジェクトのライブラリ（登録・取り込み）の全エントリーから同じ指紋の画像を探して戻す。参照先のエントリーを先に調べるが、ID が変わっていても画像が同じなら戻せる。見つからなければ `IMAGE_CHANGED` で失敗し、別の画像に置き換えない。指紋のない参照は、参照先の現在の画像を使う。
- `libref` のプレースホルダーの style で `image=lib:` を上書きした場合も同じく展開する。
- 対象は style の `image` キーだけとする。

### 4.5 展開後の検証

参照を展開した後、全ページについて次を検査する。違反はすべて報告する。

- 基盤セル、ページ内の ID（ラッパーの ID を含む）の一意性。
- `parent`・`source`・`target` の参照先が同じページにあること。`parent` が循環しないこと。
- `libref` と `image=lib:` が残っていないこと。

構造の検査は描画・レイアウトの正しさを保証しない。見た目は PNG を開いて確認する。

## 5. `write`

```
drawio-png-cli write [INPUT] -o OUTPUT [オプション]
```

処理: 入力を読む（PNG なら埋め込まれた XML を取り出す）→ ページを展開 → 参照を展開 → 検証 → フォント指定を適用 → 指定ページを描画 → 全ページの XML を PNG に埋め込む → アトミックに保存。

### 5.1 オプション

| オプション | 内容 |
| --- | --- |
| `-o`, `--output FILE` | 出力 PNG（必須）。`-` で標準出力 |
| `--page N` | 描画するページ（1 から、既定 `1`）。XML は常に全ページを埋め込む |
| `--renderer NAME` | [5.2](#52-レンダラー)。既定 `vscode` |
| `--allow-network` | 外部 HTTP(S) の画像・フォントの取得を許可する（`desktop-app` 以外） |
| `--default-font FAMILY` | `fontFamily` のないセルのフォント。XML にも保存する |
| `--fallback-font FAMILY` | 代替フォント（複数指定可）。`--default-font` が必要 |
| `--metadata-only` | 描画せず、XML だけを保存する。画素は `--base-image` から取り、なければ透明な 1×1。画像と XML が一致しない旨を警告する |
| `--base-image FILE` | `--metadata-only` と併用する |
| `--json` | 結果の要約を標準出力に出す |

`--no-validate` は廃止する。参照の展開と検証は常に行う。

### 5.2 レンダラー

v0.1.0 の `--renderer` と `--chromium-mode` を 1 つにまとめ、無効な組み合わせをなくす。既定は VS Code 拡張と同じ見た目になる `vscode` とし、draw.io Desktop の有無で結果が変わらないようにする。

| 値 | 描画 | v0.1.0 での指定 |
| --- | --- | --- |
| `vscode`（既定） | Chromium。VS Code 拡張 1.9.0（draw.io 26.0.2）互換の SVG → Canvas | `--renderer chromium --chromium-mode vscode` |
| `desktop-compat` | Chromium。draw.io Desktop 互換（DPR 2 で取得し縮小） | `--renderer chromium --chromium-mode desktop` |
| `raw` | Chromium。DPR 1 の取得結果をそのまま使う | `--renderer chromium --chromium-mode raw` |
| `desktop-app` | draw.io Desktop 27.0.2 以降のアプリ | `--renderer desktop` |

自動の切り替え（v0.1.0 の `auto`）はしない。見つからなければ `RENDERER_NOT_FOUND`。Chrome の探索、同梱資材、ネットワークの遮断、タイムアウト（60 秒）、シグナルでの中断、画素数の上限は v0.1.0 と同じ。

### 5.3 `--json` の出力

```json
{
  "output": "diagram.drawio.png",
  "width": 512, "height": 330,
  "renderer": "vscode", "page": 1, "pages": 1,
  "expanded": [
    {"page": 1, "cell": "orders", "ref": "shapes/cylinder", "type": "cells", "ids": ["orders"]},
    {"page": 1, "cell": "branch", "ref": "components/branch", "type": "cells", "ids": ["branch-1", "branch-2", "branch-3"]},
    {"page": 1, "cell": "logo", "ref": "@acme/brand/logo", "type": "image"}
  ],
  "warnings": []
}
```

## 6. `expand`

```
drawio-png-cli expand [INPUT] [-o OUTPUT] [--default-font FAMILY] [--fallback-font FAMILY] [--json]
```

参照を展開し、検証した XML を出力する。同じ入力・設定なら `write` が PNG に埋め込む XML と同じになる。

- PNG を入力すると、埋め込まれた XML をページを展開して出す。画像は実データのまま残る（v0.1.0 の `extract` と同じ）。
- draw.io・VS Code で単体で開ける XML が必要なとき、展開結果を確認したいときに使う。
- `--json` には `-o` が必要。要約は `write --json` から描画の項目を除いたもの。

## 7. `read`

```
drawio-png-cli read [INPUT] [-o OUTPUT] [--raw] [--json]
```

エージェントが編集するための XML を出力する。

- 圧縮されたページを展開する。ページ順・名前・属性・未知の要素は保持する。
- style の `image` が `data:` で始まるセル（ラッパー付き・グループの子を含む）について、指紋がプロジェクトのライブラリ（登録・取り込み）の種類 `image` のエントリーと一致すれば、値を `lib:<参照>@<指紋>`（例 `lib:@acme/brand/logo@37483e4b7f30`）に置き換える。一致が複数あれば [9.8](#98-library-search) と同じ順で最初のもの。一致しない画像は実データのまま残す。
- 既に `image=lib:` を含む XML を入力した場合はそのまま出す（何度実行しても同じ結果）。
- 置き換えた件数を標準エラーに 1 行で出し、画像を残すには `expand` を使うよう案内する。
- `--raw` は PNG に保存された XML を、ページの展開・置き換えをせずに出す（診断用）。
- `--json` には `-o` が必要。

```json
{"output":"edit.xml","pages":1,"compacted":[{"page":1,"cell":"db","ref":"simple-icons/postgresql@224afa6bb926","replacedChars":167}],"keptImages":[]}
```

## 8. `validate`

```
drawio-png-cli validate [INPUT] [--json]
```

`write` の描画前までの処理（PNG の検査、ページの展開、参照の展開、[4.5](#45-展開後の検証) の検証）を行い、ファイルを書かない。登録ライブラリを読むため、参照先の存在・種類・上書きの可否まで検査できる。CI では `validate` と `library sync --check` を使う。

```json
{"valid":true,"pages":1,"cells":14,"refs":7,"warnings":[]}
```

エラーがあれば終了コード `1` で、[2.3](#23-エラーと警告) の形式で標準エラーに出す。

## 9. ライブラリ

### 9.1 種類と探索順

ライブラリは名前空間（スコープ）の有無で 2 種類に分かれる。npm のパッケージと同じく、スコープのない名前はプロジェクトの中だけのもの、スコープ付きの名前は取得元から取り込むものとする。

| 種類（JSON の `origin`） | 名前 | 場所 | 追加の方法 |
| --- | --- | --- | --- |
| プロジェクト（`project`） | `<名前>` | `.drawio-png/libraries/<名前>.xml` | `library add`。ファイルを Git で共有する |
| 取り込み（`installed`） | `@<スコープ>/<名前>` | `.drawio-png/libraries/@<スコープ>/<名前>.xml` | `library install`。取得元は `[scopes]`、版はロックファイルで決まる |

- 名前空間が違えば別のライブラリである。`components` と `@acme/components` は共存でき、参照も別になる。
- 参照はプロジェクト領域のライブラリだけを解決する。ユーザー領域・任意のパスのライブラリファイルを直接参照する手段は設けない（同じ XML がどこでも同じ部品になるようにし、ファイルを VS Code 拡張と共有するため）。
- CLI はライブラリを同梱しない。draw.io のサイドバーの図形（AWS・Azure・UML など）を使う場合は、利用者が draw.io で使う図形をライブラリのファイルにし、`library add` で登録する（手順は [usecase.md の I](usecase.md#drawio-の図形をライブラリにする)）。図形集・アイコンのライセンスと商標の条件は、登録する利用者が確認する。
- プロジェクト領域がなければ、参照は解決できない（`REF_NOT_FOUND` で `init` を案内する）。`library list`・`library search` は何も出さない。

### 9.2 名前とエントリー ID

- ライブラリ名・スコープ名: `^[a-z0-9][a-z0-9-]{0,62}$`。
- 完全なライブラリ名は、スコープなしが `<名前>`（例 `components`）、スコープ付きが `@<スコープ>/<名前>`（例 `@acme/brand`）。コマンドの引数・出力・JSON では常に完全な名前を使う。
- エントリー ID: 小文字の英字・数字・その他の文字（日本語など）とハイフン。先頭はハイフン以外。`/`・`;`・`=`・`@`・`#`・空白・大文字は含まない。
- エントリーに `id` フィールドがあり、規則に合い重複しなければそれを使う。なければタイトルから導く: NFKC 正規化 → 小文字化 → 英数字・文字以外の連続を `-` に置換 → 前後の `-` を除去 → 空なら `entry` → 同じライブラリ内で重複すれば文書順に `-2`、`-3`… を付ける。
- 例: `PostgreSQL` → `postgresql`、`Lambda Function` → `lambda-function`、`会社ロゴ` → `会社ロゴ`、`Group⏎B` → `group-b`、`007` → `007`、空 → `entry`。
- `library add`・`library sync`・`library pack` は導いた ID を `id` フィールドに書き込み、以後タイトルが変わっても ID を変えない。

### 9.3 登録ファイルの書式

`library add`・`library sync`・`library pack` が書くファイル、`library install` が取り込むファイルは、draw.io と VS Code 拡張の両方が読める次の書式とする。

```
<mxlibrary title="Components">[
{"id":"service","title":"Service","w":120,"h":60,"xml":"\u003cmxGraphModel\u003e\u003croot\u003e…"},
{"id":"card","title":"Card","w":240,"h":120,"xml":"\u003cmxGraphModel\u003e…"}
]</mxlibrary>
```

1. ルートは `<mxlibrary>`。`title` 属性に表示名を入れる。XML 宣言・CDATA・コメントは書かない。
2. 要素の中身は JSON 配列だけ。JSON 文字列の `<`・`>`・`&` は `\u003c`・`\u003e`・`\u0026` で書き、XML の実体参照（`&lt;` など）が一つも現れないようにする。
3. 1 行に 1 エントリー。キーは `id`・`title`・`w`・`h`・`aspect`（あれば）・`xml` または `data`、その後に未知のキーを元の順で並べる。
4. `xml` の圧縮・非圧縮はソースのまま保持する。`data` の URI も変換しない（変換は展開時に行う）。
5. UTF-8、改行は LF、末尾に改行。

2 と 1 の CDATA の禁止は、VS Code 拡張 1.9.0 が要素の最初の子のテキストを実体参照を解かずに `JSON.parse` するためである（[vscode.md](vscode.md#3-ライブラリファイルの書式の制約)）。draw.io は XML として解析するので、どちらでも同じ文字列が得られる。

### 9.4 エントリーの種類

基盤セル・レイヤーは数えない。ラッパーと内側の `<mxCell>` は 1 セルと数える。最上位のセルは、親がエントリーのレイヤーであるセル。

| 種類 | 条件 | `libref` | `image=lib:` | 幅・高さ | ラベル・style・独自データ |
| --- | --- | --- | --- | --- | --- |
| `image` | 1 セルの図形で style の `image` が `data:`、または `data` のエントリー（`data:` URI） | 可 | 可 | 可 | 可 |
| `shape` | 1 セルの図形（`image` 以外） | 可 | 不可 | 可 | 可 |
| `edge` | 1 セルの接続線 | 可（`source`・`target`） | 不可 | 不可 | 可 |
| `group` | 最上位が 1 つの図形で、子孫のセルがある | 可 | 不可 | 不可 | 可（最上位のセル） |
| `set` | 最上位のセルが 2 つ以上 | 可（ID は `-n`） | 不可 | 不可 | 不可 |

`data` が HTTP(S) の URL のエントリーは `shape` とし、描画には `--allow-network` が必要になる。解析できないエントリーは `invalid` とし、検索結果に警告付きで出すが参照できない。

### 9.5 スコープと取得元

スコープは、ライブラリの取得元（配布場所）に付ける名前空間である。`config.toml` の `[scopes]` で、スコープごとに取得元を 1 つ決める（npm の `.npmrc` の `@scope:registry=` に当たる）。

| 取得元 | 書き方の例 | 取得の方法 |
| --- | --- | --- |
| ディレクトリ | `"file:../drawio-libraries"`、`"file:~/drawio-libraries"` | ファイルを読む。相対パスはプロジェクトのルート（`.drawio-png/` の親。ユーザーの設定では `DRAWIO_PNG_HOME`）から |
| Git | `"git+https://github.com/acme/drawio-libraries.git"`、`"git+ssh://git@github.com/acme/drawio-libraries.git#release"` | `git` コマンドで取得する。`#` の後はブランチまたはタグ（省略時は既定のブランチ）。認証は Git の設定（認証ヘルパー・SSH 鍵）に任せる |
| HTTPS | `{ url = "https://libs.example.com/drawio/", token-env = "PARTNER_LIBS_TOKEN" }` | `curl` コマンドで HTTPS から取得する。`token-env` があれば、その環境変数の値を `Authorization: Bearer` で送る（コマンドラインには出さない） |

取得元の中身は、種類によらず次の構成とする。1 つの取得元に複数のスコープを置ける。CLI は設定したスコープの `@<スコープ>/` の下だけを読む。

```
<取得元>/
  @acme/
    brand/
      versions.json        # 版の一覧
      1.0.0.xml            # 版ごとのライブラリ（9.3 の書式）
      1.1.0.xml
```

`versions.json`（[例](examples/acme-drawio-libraries/@acme/brand/versions.json)）:

```json
{
  "name": "@acme/brand",
  "title": "ACME Brand",
  "versions": {
    "1.0.0": {"file": "1.0.0.xml", "integrity": "sha256-sBOSRnQChE4BD7t9wykTJVDR5nb4zAIvz/Jkp2UBBYI=", "entries": 1}
  }
}
```

- 版は `MAJOR.MINOR.PATCH`。プレリリース版（`1.0.0-beta` など）は扱わない。
- `integrity` は版のファイルのバイト列の SHA-256 を Base64 で表したもの（`sha256-<Base64>`。npm のロックファイル・SRI と同じ書式）。
- 公開した版のファイルは変えない。中身が `integrity` と合わなければ取り込まない（`LIBRARY_INTEGRITY`）。
- 版のファイルは 9.3 の書式でなければ取り込まない（`LIBRARY_INVALID`）。VS Code 拡張で読めないファイルをプロジェクトに入れないためである。この構成のファイルは `library pack`（[9.14](#914-library-pack)）で作る。
- 取得元への公開（ファイルの配置・`git push`・Web サーバーへのアップロード）は、それぞれの手段で行う。CLI は公開・認証・権限の管理を扱わない。
- 取得元に接続するのは `library install`・`library update` だけである。`write` などの他のコマンドはネットワークに接続しない。
- トークンは、`https:` 以外の URL と、設定したホストと別のホストへのリダイレクト先には送らない。
- スコープ `drawio` には取得元を設定できない。参照・取り込みで使ったスコープに取得元がなければ `SCOPE_UNKNOWN`。

### 9.6 依存関係とロックファイル

プロジェクトが使うスコープ付きのライブラリは、`config.toml` の `[dependencies]` に版の範囲と一緒に書き、実際に取り込んだ版を `lock.json` に記録する（npm の `package.json` の `dependencies` と `package-lock.json` に当たる）。

```toml
[dependencies]
"@acme/brand" = "^1.0.0"
```

| 範囲 | 意味 |
| --- | --- |
| `1.2.3` | その版だけ |
| `^1.2.3` | `1.2.3` 以上 `2.0.0` 未満（`0.y.z` では `0.y` が同じもの） |
| `~1.2.3` | `1.2.3` 以上 `1.3.0` 未満 |
| `*` | すべての版 |

`lock.json`（Git の取得元の場合。`file:` の取得元を使った例は [examples/project/.drawio-png/lock.json](examples/project/.drawio-png/lock.json)）:

```json
{
  "lockfileVersion": 1,
  "libraries": {
    "@acme/brand": {
      "version": "1.0.0",
      "resolved": "git+https://github.com/acme/drawio-libraries.git#<コミット>",
      "integrity": "sha256-sBOSRnQChE4BD7t9wykTJVDR5nb4zAIvz/Jkp2UBBYI="
    }
  }
}
```

- `resolved` は取得元。Git では取得したコミットを `#` の後に書き、次回からはそのコミットから取得する。
- `config.toml` と `lock.json` は Git にコミットする。取り込んだファイル（`.drawio-png/libraries/@*/`）は `init` が作る `.gitignore` で除外し、clone した後や CI では `library install` で復元する（npm の `node_modules` と同じ扱い）。
- 参照を解決するコマンド（`write`・`expand`・`validate`・`read`・`library search` など）は、使うライブラリのファイルを `lock.json` の完全性と照合する。合わなければ `LIBRARY_INTEGRITY`、ファイルがなければ `LIBRARY_NOT_INSTALLED` とし、どちらも `library install` を案内する。取得元には接続しない。
- 保存済みの PNG は参照を含まないため、ライブラリの版を上げても既存の図面は変わらない。影響を受けるのは、これから `write` する XML の参照と、`read` が出した指紋付きの画像参照（画像が変わっていれば `IMAGE_CHANGED`）だけである。
- 依存関係は 1 段だけで、ライブラリが別のライブラリに依存する仕組みはない。

### 9.7 `library list`

```
drawio-png-cli library list [--scope SCOPE] [--json]
```

テキスト: `名前<TAB>版<TAB>エントリー数<TAB>表示名`（版はスコープ付きのライブラリだけ）。プロジェクト → 取り込みの順で、それぞれ名前順。`--scope acme` でそのスコープのライブラリだけを出す。

```json
[{"name":"simple-icons","origin":"project","title":"Simple Icons","entries":3466,"file":".drawio-png/libraries/simple-icons.xml","normalized":true},
 {"name":"@acme/brand","origin":"installed","version":"1.0.0","range":"^1.0.0","title":"ACME Brand","entries":1,"file":".drawio-png/libraries/@acme/brand.xml","normalized":true}]
```

`normalized` は 9.3 の書式どおりか。`false` なら `library sync` を案内する。依存にあって取り込まれていないライブラリは `"installed":false` を付けて出す。

### 9.8 `library search`

```
drawio-png-cli library search [QUERY...] [--library NAME] [--scope SCOPE] [--kind KIND] [--limit N] [--json]
```

- 語を空白で区切り、すべての語がタイトルか ID に含まれる（大文字小文字を区別しない）エントリーを出す。語を省略すると全エントリー。
- 順序: タイトル・ID の完全一致 → 前方一致 → 部分一致。同順位はプロジェクト → 取り込み、ライブラリ名順、エントリー順。
- `--limit` の既定は `50`（`0` で無制限）。打ち切った場合は全件数を標準エラーに出す。
- テキスト: `参照<TAB>種類<TAB>幅x高さ<TAB>タイトル`（タイトルの改行・タブは空白にして表示）。先頭の列をそのまま XML に写せる。

```json
[{"ref":"simple-icons/postgresql","library":"simple-icons","id":"postgresql","title":"PostgreSQL","kind":"image","width":48,"height":48,"origin":"project"},
 {"ref":"@acme/brand/logo","library":"@acme/brand","id":"logo","title":"Logo","kind":"image","width":240,"height":120,"origin":"installed","version":"1.0.0"}]
```

画像データ・style は出さない。

### 9.9 `library show`

```
drawio-png-cli library show REF [--json]
```

`REF` がライブラリ名ならライブラリの詳細（`library list` の 1 件分）、参照文字列ならエントリーの詳細を出す。

```json
{
  "ref": "components/card", "title": "Card", "kind": "group",
  "width": 240, "height": 120, "origin": "project",
  "label": "Card", "style": "group;",
  "cells": [
    {"id": "<ID>",   "kind": "vertex", "label": "Card",     "parent": null},
    {"id": "<ID>-1", "kind": "vertex", "label": "タイトル", "parent": "<ID>"},
    {"id": "<ID>-2", "kind": "vertex", "label": "本文",     "parent": "<ID>"}
  ],
  "example": "<mxCell id=\"ID\" parent=\"1\" libref=\"components/card\"><mxGeometry x=\"0\" y=\"0\" as=\"geometry\"/></mxCell>"
}
```

- `cells` は展開後の ID の付き方（`<ID>` はプレースホルダーの ID）と各セルの種類・ラベル・親を示す。
- `style` は最上位のセルが 1 つのときだけ出す（v0.1.0 の `library style` の代わり）。最上位のセルがラッパーで包まれていれば、その独自データを `data`（例 `{"owner":"library","note":"keep"}`）に出す。
- 種類 `image` には `"image":{"fingerprint":"224afa6bb926","mediaType":"image/svg+xml","bytes":109}` を付ける（`bytes` はデコードした画像の大きさ）。画像データは出さない。
- 取り込んだライブラリのエントリーには `version` を付ける。
- `example` は種類に合わせたプレースホルダーの例（`image`・`shape` は幅・高さ付き、`edge` は `source`・`target` 付き）。

### 9.10 `library preview`

```
drawio-png-cli library preview [QUERY...|REF...] -o PNG [--library NAME] [--scope SCOPE] [--kind KIND] [--allow-network]
```

該当するエントリーを、参照文字列とタイトル付きで並べた PNG を `vscode` レンダラーで描く。`/` を含む引数と `@` で始まる引数は参照文字列（ライブラリ名ならその全エントリー）、それ以外は検索語として扱う。60 件を超える場合は絞り込みを求めるエラーにする。

### 9.11 `library add`

```
drawio-png-cli library add SOURCE... [--name NAME] [--title TEXT]
                                     [--replace | --merge] [--skip-invalid] [--json]
```

プロジェクト専用のライブラリ（スコープなし）を登録する。

| SOURCE | 扱い |
| --- | --- |
| `<mxlibrary>` の XML ファイル | エントリーを写す。CDATA・実体参照・圧縮のいずれの形式も読む |
| 画像ファイル（PNG・JPEG・GIF・SVG・WebP） | 1 ファイルを 1 つの `data` エントリー（`data:<type>;base64,…`、`aspect` は `fixed`）にする。タイトルはファイル名から拡張子を除いたもの。幅・高さは画像から読み、読めなければ `LIBRARY_INVALID` |
| `-` | 標準入力の `<mxlibrary>` |

- 登録先は `.drawio-png/libraries/<名前>.xml`。プロジェクト領域が見つからなければ `PROJECT_NOT_FOUND`（`init` を案内する）。
- `--name` の既定は最初の SOURCE のファイル名（拡張子を除く）を 9.2 の規則で整えたもの。スコープ付きの名前（`@acme/brand`）は指定できない（`USAGE`。`library pack` と `library install` を案内する）。`--title` の既定はソースの `title` 属性、なければ名前。
- 同名のライブラリがあれば `LIBRARY_EXISTS`。`--replace` で全体を置き換え、`--merge` で同じ ID のエントリーを置き換えて新しいエントリーを末尾に追加する。
- エントリーを検査し、`xml` が `<mxGraphModel>` でない、`data` が画像の URI でないなど解析できないものがあれば `LIBRARY_INVALID` で何も書かない。`--skip-invalid` で除外して登録する。
- URL を SOURCE に取らない。ネットワークからは取得しない。
- 出力: 名前・ファイル・エントリー数・各エントリーの参照文字列と種類。`--json` では配列で出す。最後に `library vscode --write` を案内する。

### 9.12 `library install`

```
drawio-png-cli library install [NAME[@範囲]...] [--frozen] [--offline] [--json]
```

スコープ付きのライブラリを取得元から取り込む（npm の `npm install`）。

- 引数なし: `[dependencies]` のすべてを取り込む。`lock.json` にあり範囲に合う依存はその版を使い、ない・合わない依存は範囲に合う最新の版を選んで `lock.json` に書く。依存から外れた取り込み済みのファイルは消す。
- 引数あり（例 `@acme/brand`、`@acme/brand@^1.2.0`）: `[dependencies]` に追加・更新して取り込む。範囲を省略すると、最新の版 `X.Y.Z` を取り込み `^X.Y.Z` と記録する。
- `--frozen`: `config.toml`・`lock.json` を変えない。依存と `lock.json` が合わなければ `LOCK_OUTDATED` で何もしない（CI 用。npm の `npm ci`）。
- `--offline`: 取得元に接続せず、キャッシュ（2.2）だけを使う。
- 取得したファイルは `versions.json` の完全性と照合し、9.3 の書式であることを確かめてから `.drawio-png/libraries/@<スコープ>/<名前>.xml` に置く。1 つでも失敗したら、どのファイルも `config.toml`・`lock.json` も変えない。
- 名前はスコープ付きだけ。スコープなしの名前は `USAGE`（`library add` を案内する）。
- 出力: 追加・更新・削除したライブラリの名前・版・エントリー数。VS Code の設定にないライブラリがあれば `library vscode --write` を案内する。

```
installed @acme/brand@1.0.0 (1 entry) from file:../acme-drawio-libraries
  @acme/brand/logo	image	240x120	Logo
hint: run `drawio-png-cli library vscode --write` to use it in VS Code
```

### 9.13 `library update`

```
drawio-png-cli library update [NAME...] [--latest] [--dry-run] [--json]
```

- 取り込んだライブラリ（省略時はすべて）を、範囲内の最新の版にして `lock.json` を書き換える（npm の `npm update`）。
- `--latest`: 範囲を超えて最新の版にし、範囲を `^<版>` に書き換える。
- 出力に版の変化と、新しい版でなくなったエントリーの参照（`removed`）を出す。なくなったエントリーを参照する XML は `REF_NOT_FOUND` になる。
- `--dry-run`: 取得元に接続して変化だけを出し、何も書かない。

### 9.14 `library pack`

```
drawio-png-cli library pack SOURCE... --name @SCOPE/NAME --version X.Y.Z --to DIR [--title TEXT] [--skip-invalid] [--json]
```

取得元に置くライブラリの版を作る。ライブラリを配布する側が使う。

- SOURCE は `library add` と同じ（`<mxlibrary>` の XML・画像ファイル・`-`）。
- `DIR/@SCOPE/NAME/X.Y.Z.xml` を 9.3 の書式で書き、`DIR/@SCOPE/NAME/versions.json` にその版を追加する（なければ作る）。`DIR` は取得元のディレクトリ（`file:` のディレクトリ、Git の作業ツリー、Web サーバーに置くファイル一式）。
- `--title` の既定は前の版の表示名、なければソースの `title` 属性、なければ名前。
- エントリー ID は 9.2 の規則で決め、前の版にあった ID のエントリーはその ID を保つ。前の版と比べてなくなった ID・増えた ID を出力する。なくなった ID があるのにメジャー版を上げていなければ警告 `ENTRY_REMOVED` を出す。
- 同じ版が既にあれば `VERSION_EXISTS` で何も書かない。公開した版は変えない。
- 取得元への公開（`git commit`・`git push`、アップロード）は利用者が行う。

### 9.15 `library remove`

```
drawio-png-cli library remove REF [--json]
```

| `REF` | 処理 |
| --- | --- |
| スコープなしのライブラリ名 | ファイルを消す |
| スコープなしの参照文字列 | そのエントリーを消す |
| スコープ付きのライブラリ名 | `[dependencies]`・`lock.json`・取り込んだファイルから消す（npm の `npm uninstall`） |
| スコープ付きの参照文字列 | `LIBRARY_MANAGED`。取得元で新しい版を作る |

保存済みの PNG は実画像を持つため影響を受けない。`read` 後の `image=lib:` は、同じ画像が他のエントリーになければ `write` で `IMAGE_CHANGED` になる。

### 9.16 `library sync`

```
drawio-png-cli library sync [NAME...] [--check] [--json]
```

draw.io Desktop のライブラリ編集や手作業で変わったプロジェクト専用のライブラリを 9.3 の書式に戻し、`id` のないエントリーに ID を付ける。既存の `id` は変えない。取り込んだライブラリは書き換えない（`LIBRARY_MANAGED`）。

`--check` は書き込まず、正規化が必要なファイル、取り込まれていない・完全性の合わないライブラリがあれば終了コード `1`（CI 用）。

### 9.17 `library vscode`

```
drawio-png-cli library vscode [--entry-id TEXT] [--write] [--json]
```

VS Code 拡張の `hediet.vscode-drawio.customLibraries` の値を出力する。詳細は [vscode.md](vscode.md)。

- プロジェクト専用のライブラリと取り込んだライブラリを出す。`file` は `${workspaceFolder}/<プロジェクトからの相対パス>`。ワークスペースのフォルダーはプロジェクト領域の親ディレクトリとみなす。
- `entryId` は、プロジェクト専用のライブラリが `drawio-png-cli`、取り込んだライブラリが `drawio-png-cli @<スコープ>`。拡張はこの値を見出しにしてライブラリをまとめ、見出しごとに表示・非表示を切り替えられる。`--entry-id` で `drawio-png-cli` の部分を変えられる。
- `libName` はライブラリの表示名。
- `--write`: `<プロジェクト>/.vscode/settings.json` の `hediet.vscode-drawio.customLibraries` のうち、`entryId` が `drawio-png-cli` か `drawio-png-cli @` で始まる項目だけを置き換える。他の項目・設定・コメントは保持する。ファイルがなければ作る。JSONC として解析できなければ何も書かずにエラーにする。

## 10. `init`

```
drawio-png-cli init [DIR] [--json]
```

`DIR`（既定はカレントディレクトリ）に `.drawio-png/libraries/`、コメントだけの `config.toml`、取り込んだライブラリを除外する `.gitignore` を作る。既にあれば何も変えずに成功する。`lock.json` は最初の `library install` で作る。

## 11. `skill`・`licenses`

- `skill`: この版のエージェント向け手順書を出す（v0.1.0 の `--full` は廃止し 1 種類にする）。手順書は「XML を書いて `write` に渡す」「部品は `library search` → `libref`（参照文字列は検索結果からそのまま写し、スコープを推測で補わない）」「既存の PNG は `read` → 編集 → `write`」「PNG を開いて確認する」を基本の流れとする。
- `licenses`: v0.1.0 と同じ。

## 12. v0.1.0 からの変更

| v0.1.0 | v0.2.0 |
| --- | --- |
| `embed -i X -o Y` / 標準入力 | `write X -o Y` / `write -o Y` |
| `embed --no-render [-b BASE]` | `write --metadata-only [--base-image BASE]` |
| `embed --no-validate` | 廃止 |
| `--renderer auto\|desktop\|chromium`・`--chromium-mode` | `--renderer vscode\|desktop-compat\|raw\|desktop-app`（既定 `vscode`） |
| `extract`（画像を残す） | `expand`（画像を残す）。登録画像を `image=lib:` に縮めて取り出すのは `read` |
| `validate`（構造のみ） | `validate`（参照・ID・接続先も検査） |
| `insert` | 廃止。XML の `libref` で書く |
| `library style LIB INDEX` | `library show REF` の `style` |
| `library search` の番号 | 参照文字列 |
| `DRAWIO_PNG_LIBRARY_PATH`・`--library-file`・`--no-builtin` | 廃止。プロジェクト専用は `library add`、共有するものはスコープを付けて `library install` で取り込む。検索は `--library`・`--scope` で絞る |
| 組み込みライブラリ（`drawio/aws4-compute` など） | 廃止。使う図形を draw.io でライブラリにして `library add` で登録する |
| `DRAWIO_PNG_CHROMIUM_MODE` | `DRAWIO_PNG_RENDERER` |
| `skill --full` | `skill` |

計画中だった PRD 4.2 の `drawioPng*` 属性・`image=drawio-png`・画像ハッシュによる参照は、`libref`・`image=lib:` に置き換える。登録により ID が安定するため、タイトル・番号・ハッシュを組み合わせた指定は不要になる。

## 13. 未決定事項

- `set` の展開でプレースホルダーの ID を残すために、グループで包むか（現案は包まず `-n` を付ける）。
- `group` の幅・高さの上書き（子セルの比例配置）を許すか。
- 既存の図面のセルをエントリーとして登録する機能（`library add diagram.drawio --cell ID`）。
- リモート環境（devcontainer・WSL）での VS Code 拡張によるライブラリの読み込み（[vscode.md](vscode.md#7-注意と未検証の事項)）。
- 指紋の長さ（12 桁）の妥当性。
- 取り込んだライブラリを Git にコミットするか（現案は npm と同じく `.gitignore` で除外し、`library install` で復元する。コミットすれば CI・VS Code で取得元への接続が要らなくなるが、リポジトリが大きくなる）。
- 版の範囲の書き方（現案は完全一致・`^`・`~`・`*` だけ）とプレリリース版、公開した版の取り下げ（npm の `deprecate`・`unpublish`）。
- 同じエントリー ID のまま中身が変わったことを `library update` で知らせるか（現案は ID の増減だけを出す）。
- 公開・認証を CLI で扱うレジストリーサーバー（現案は扱わず、取得元ごとの手段で公開する）。

## 14. 確認したこと・していないこと

- VS Code 拡張 1.9.0（タグ `v1.9.0`）のソースで、`customLibraries` の読み込み方法と `extensionKind` を確認した。拡張が使う `xml-parser-xo` 3.1.0 と同じ手順で、例のライブラリと実物の Simple Icons for draw.io 16.35.0 を読み、CDATA 形式が失敗し、CLI の書式が成功することを確かめた。実体参照を使った書式は、パーサーのソースから読めないと判断した（実行はしていない）。
- draw.io 26.0.2 の `app.min.js` と 31.4.6 のソースで、`data` エントリーの画像の変換（`convertDataUri`）とライブラリの保存処理を確認した。
- B〜E・G の「CLI がない場合の XML」は、0.1.0 系の CLI で `validate` を通し、Chrome の vscode モード（draw.io 26.0.2 の Web 資材）で描画した（`expected.drawio.png`）。利用者ライブラリの部品は 0.1.0 系の `insert` でも展開し、ID・位置・サイズ・画像の値が一致することを確かめた。A・F・H は v0.2.0 の CLI の `write`（vscode、同梱資材のみ）で描画した。全ケースの展開結果は `tests/acceptance.rs` で検査している。
- 名前空間の例（[examples/acme-drawio-libraries/](examples/acme-drawio-libraries/)・[lock.json](examples/project/.drawio-png/lock.json)）は、`library pack`・`library install` が書くファイルとバイト単位で一致することを `tests/install.rs` で検査している。npm との対応は仕様上の対応である。
- テキストの出力例（`library search`・`add`・`install`・`pack`・`update`、エラー表示）は実行結果と照合した。`--json` の例（9.7〜9.9 など）は仕様から作ったもので、キーの順序・数値の書き方・一部の項目が実行結果と異なる。VS Code 上での操作、リモート環境（devcontainer・WSL）での拡張の動作は確認していない。
