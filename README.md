# drawio-png-cli

`drawio-png-cli` は、draw.io の XML を標準入力で受け取り、VS Code の draw.io 拡張で開いて編集できる `.drawio.png` を作る Rust 製 CLI です。AI エージェントは図面全体を XML で書き、図形・アイコンはライブラリの参照文字列（`libref="simple-icons/postgresql"` など）で指定できます。既存の `.drawio.png` から編集用の XML を取り出し、検証してから書き戻せます。

> **このブランチは v0.2.0 の開発中です。** この README は v0.2.0 の仕様（[docs/v0.2.0_claude/](docs/v0.2.0_claude/)）に合わせて書いており、記載したコマンドの多くはまだ実装されていません。現在の CLI の使い方は [main ブランチの README](https://github.com/szk302/drawio-png-cli/blob/main/README.md) を参照してください。v0.1.0 からの変更は [仕様の 12 章](docs/v0.2.0_claude/spec.md#12-v010-からの変更) にまとめています。

## インストール

[GitHub Releases](https://github.com/szk302/drawio-png-cli/releases) から、OS に合ったアーカイブを取得して展開し、`drawio-png-cli`（Windows は `drawio-png-cli.exe`）を PATH の通った場所に置きます。アーカイブにはライセンス通知一式を同梱しています。

| OS | アーカイブ |
| --- | --- |
| Linux x86_64 / ARM64 | `drawio-png-cli-<version>-x86_64-unknown-linux-musl.tar.gz` / `drawio-png-cli-<version>-aarch64-unknown-linux-musl.tar.gz`（静的リンク） |
| macOS Apple silicon / Intel | `drawio-png-cli-<version>-aarch64-apple-darwin.tar.gz` / `drawio-png-cli-<version>-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `drawio-png-cli-<version>-x86_64-pc-windows-msvc.zip` |

```sh
# 例: Linux x86_64。SHA256SUMS で改ざんを確認する
version=0.2.0
gh release download "v$version" -R szk302/drawio-png-cli \
  -p "drawio-png-cli-$version-x86_64-unknown-linux-musl.tar.gz" -p SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS
# GitHub Actions でビルドされたことの確認（任意）
gh attestation verify "drawio-png-cli-$version-x86_64-unknown-linux-musl.tar.gz" -R szk302/drawio-png-cli
tar xzf "drawio-png-cli-$version-x86_64-unknown-linux-musl.tar.gz" drawio-png-cli
```

macOS のバイナリは署名・公証していません。ブラウザーで取得した場合は、初回実行時に Gatekeeper の警告が出ることがあります（`xattr -d com.apple.quarantine drawio-png-cli` で解除できます）。

ソースからビルドする場合、開発環境の Rust は `mise.toml` で固定しています。

```sh
mise install
mise exec -- cargo install --path . --locked
```

Rust を直接管理している場合は `cargo install --path . --locked` でもインストールできます。実行ファイル名は `drawio-png-cli` です。Node.js や Python は不要です。PNG の描画には Chromium / Chrome（`--renderer desktop-app` では draw.io Desktop 27.0.2 以降）が必要です。Chromium 用の基本描画資材はバイナリに同梱しています。`library install` で Git の取得元を使う場合は `git`、HTTPS の取得元を使う場合は `curl` が必要です。

## 使い方

```sh
# プロジェクトを用意し、ライブラリを登録・取り込む（初回のみ）
drawio-png-cli init                                   # .drawio-png/ を作る
drawio-png-cli library add ./simple-icons.xml         # プロジェクト専用のライブラリを登録する
drawio-png-cli library install @acme/brand            # 社内の共有ライブラリを取り込む（取得元は .drawio-png/config.toml）
drawio-png-cli library vscode --write                 # VS Code 拡張でも同じライブラリを使う

# 部品を探す。先頭の列が XML に書く参照文字列
drawio-png-cli library search postgres
#   simple-icons/postgresql	image	48x48	PostgreSQL

# XML を標準入力で渡して PNG を作る
drawio-png-cli write -o arch.drawio.png < arch.xml

# 既存の PNG を編集する（登録済みの画像は image=lib: の参照に縮めて出す）
drawio-png-cli read diagram.drawio.png -o edit.xml
drawio-png-cli write edit.xml -o diagram.drawio.png

# 参照を展開した単体の XML を出す（draw.io・VS Code でそのまま開ける）
drawio-png-cli expand diagram.drawio.png -o diagram.drawio

# 検証だけ行う（何も書かない）
drawio-png-cli validate diagram.xml
```

XML の中では、部品を `libref` のプレースホルダーで書きます。`write` は参照を展開し、検証・描画してから、参照を含まない普通の draw.io XML を PNG に埋め込みます。

```sh
drawio-png-cli write -o arch.drawio.png <<'EOF'
<mxfile>
  <diagram id="page-1" name="構成図">
    <mxGraphModel>
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <mxCell id="fn" value="Handler" parent="1" libref="@drawio/aws4-compute/lambda">
          <mxGeometry x="40" y="40" as="geometry"/>
        </mxCell>
        <mxCell id="db" value="DB" parent="1" libref="simple-icons/postgresql">
          <mxGeometry x="200" y="55" width="48" as="geometry"/>
        </mxCell>
        <mxCell id="e" style="endArrow=block;html=1;" edge="1" parent="1" source="fn" target="db">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
EOF
```

作業例と、CLI を使わない場合に書く XML との比較は [ユースケース](docs/v0.2.0_claude/usecase.md) を参照してください。

## コマンド一覧

各コマンドの詳細は `drawio-png-cli <コマンド> --help` と [仕様](docs/v0.2.0_claude/spec.md) で確認できます。

| コマンド | 内容 |
| --- | --- |
| `read [INPUT] [-o XML]` | 編集用の XML を出す。圧縮ページを展開し、登録済みの画像を `image=lib:` の参照に縮める |
| `write [INPUT] -o PNG` | 参照を展開し、検証・描画して、全ページの XML を `.drawio.png` に保存する |
| `expand [INPUT] [-o XML]` | 参照を展開した XML を出す。`write` が埋め込むものと同じ |
| `validate [INPUT]` | `write` の描画前までの処理（展開・検証）を行い、何も書かない |
| `init [DIR]` | プロジェクト領域 `.drawio-png/` を作る |
| `library list` | ライブラリの一覧 |
| `library search [QUERY...]` | エントリーを検索し、参照文字列・種類・サイズ・タイトルを出す |
| `library show REF` | ライブラリ・エントリーの詳細（style・セル構成・配置例） |
| `library preview QUERY... -o PNG` | 該当するエントリーを並べた PNG を描く（最大 60 件） |
| `library add SOURCE...` | ライブラリ・画像ファイルを、プロジェクト専用のライブラリとして登録する |
| `library install [NAME[@範囲]...]` | スコープ付きのライブラリを取得元から取り込む |
| `library update [NAME...]` | 取り込んだライブラリを新しい版にする |
| `library pack SOURCE... --to DIR` | 取得元に置くライブラリの版を作る |
| `library remove REF` | ライブラリ・エントリーの登録、取り込みを消す |
| `library sync [NAME...]` | 人が編集したライブラリファイルを正規化する |
| `library vscode` | VS Code 拡張の設定を出力・更新する |
| `skill` | AI エージェント向けの手順書を出す |
| `licenses` | drawio-png-cli・依存クレート・同梱資材のライセンスと通知を出す |

`read`・`write`・`expand`・`validate` は、XML（`<mxfile>`・`<mxGraphModel>`）と `.drawio.png` のどちらも受け付け、拡張子ではなく内容で判別します。

## 共通の規約

| 規約 | 内容 |
| --- | --- |
| 図面の入力 | 位置引数 `INPUT` を 1 つ取る。省略または `-` で標準入力 |
| 出力先 | `-o`, `--output`。XML・JSON・表は省略時に標準出力。PNG は必須で、`-o -` を明示した場合だけ標準出力に書く |
| 上書き | 出力先は入力と同じパスでもよい。同じディレクトリの一時ファイルに書き、同期後にアトミックに置き換える。失敗時は既存の出力を変更しない。出力先ディレクトリは作成しない |
| 機械向け出力 | `--json` は全コマンド共通で、標準出力に JSON 値を 1 つだけ出す。XML を標準出力に出すコマンドで `--json` を使う場合は `-o` が必要 |
| 診断 | 警告・エラーは標準エラー。入力 XML の問題は最初の 1 件で止めず、コード付きでまとめて報告する（`REF_NOT_FOUND` では近い参照を候補に示す） |
| 終了コード | `0` 成功、`1` 入力・参照・描画・保存のエラー、`2` 引数のエラー |
| 対話 | 確認の問い合わせはしない |

描画中に SIGINT（Ctrl+C）・SIGTERM・SIGHUP（Windows では Ctrl+C・Ctrl+Break）を受けると、描画を中断してレンダラーのプロセスと一時ファイルを片付け、既存出力を変更せずにそのシグナルで終了します（シェルでの終了コードは `128 + シグナル番号`、SIGINT なら `130`）。描画以外の処理中は通常どおり直ちに終了し、`nohup` などで無視されているシグナルは無視したままです。SIGKILL などの強制終了では片付けられません。

### `drawio-png-cli write`

| オプション | 内容 |
| --- | --- |
| `-o`, `--output FILE` | 出力 PNG（必須）。`-` で標準出力 |
| `--page N` | 描画するページ（1 から、既定 1）。XML は常に全ページを埋め込む |
| `--renderer vscode\|desktop-compat\|raw\|desktop-app` | 描画方式。既定は `vscode`（[レンダラーの選択](#レンダラーの選択)） |
| `--allow-network` | 外部 HTTP(S) の画像・フォントの取得を許可する（`desktop-app` 以外） |
| `--default-font FAMILY` | `fontFamily` のないセルのフォント。XML にも保存する |
| `--fallback-font FAMILY` | 代替フォント（複数指定可）。`--default-font` が必要 |
| `--metadata-only` | 描画せず、XML だけを保存する。画素は `--base-image` から取り、なければ透明な 1×1 PNG |
| `--base-image FILE` | `--metadata-only` と併用する |
| `--json` | 展開した参照・描画の要約を標準出力に出す |

参照の展開と検証は常に行います。`--metadata-only` は画像と XML の見た目が一致しない旨を警告します。

### `read`・`expand`・`validate`

- `read` は圧縮ページを展開し、style の `image` がプロジェクトのライブラリの画像と同じなら、値を指紋付きの参照（`lib:simple-icons/postgresql@224afa6bb926`）に縮めます。Base64 の画像をエージェントのコンテキストに入れずに編集でき、`write` で元の画像に戻ります。ライブラリにない画像は実データのまま残します。`--raw` は PNG に保存された XML をそのまま出します（診断用）。
- `expand` は参照を展開して検証した XML を出します。PNG を渡すと埋め込まれた XML を画像を含めて出すため、draw.io・VS Code で単体で開くファイルやバックアップに使えます。
- `validate` は PNG の検査、ページと参照の展開、ID・親子関係・接続先の検査を行い、何も書きません。CI では `validate` と `library sync --check` を使います。

## XML の参照

部品は参照文字列で指定します。

| 参照文字列 | 意味 |
| --- | --- |
| `components/service` | プロジェクトに登録したライブラリ `components` のエントリー `service` |
| `@acme/brand/logo` | スコープ `acme` の取得元から取り込んだライブラリ `@acme/brand` のエントリー `logo` |
| `@drawio/aws4-compute/lambda` | 組み込みライブラリ `@drawio/aws4-compute` のエントリー `lambda` |

| 書き方 | 展開されるもの |
| --- | --- |
| セルの属性 `libref="<参照>"` | エントリーのセル群（style・ラベル・独自データ・子セル・接続線）。プレースホルダーの位置・ラベル・style・サイズなどで上書きできる |
| style の `image=lib:<参照>` | `image` の値（実画像）だけ。ラベル・サイズ・他の style は書いたとおりに残る |

- 参照文字列は `library search` の結果からそのまま写します。タイトルや番号では指定しません。
- 複数セルのエントリーは、最上位のセルがプレースホルダーの ID を受け継ぎ、子孫が `<ID>-1`、`<ID>-2`… になります。展開後の ID は `library show REF --json` の `cells` で事前に分かります。
- 展開したセルには参照が残りません。PNG に保存されるのは普通の draw.io XML で、開くのに CLI もライブラリも要りません。

上書きできる項目・ID の規則・接続線の扱いは [仕様の 4 章](docs/v0.2.0_claude/spec.md#4-図面-xml-と参照) を参照してください。

## ライブラリ

ライブラリは draw.io のカスタムライブラリ（`<mxlibrary>` 形式の XML）です。参照できるのはプロジェクトに登録・取り込みしたライブラリと組み込みライブラリだけで、任意のパスのファイルや環境変数で指したファイルは使いません。同じ XML が、どの環境でも同じ部品に展開されるようにするためです。

ライブラリ名には npm と同じ名前空間（スコープ）があります。

| 種類 | 名前 | 追加の方法 | 置き場所 |
| --- | --- | --- | --- |
| プロジェクト専用 | `components` | `library add` | `.drawio-png/libraries/components.xml`（Git にコミットする） |
| 取り込み | `@acme/brand` | `library install` | `.drawio-png/libraries/@acme/brand.xml`（Git から除外し、`lock.json` から復元する） |
| 組み込み | `@drawio/aws4-compute` | 不要 | drawio-png-cli に同梱 |

```sh
drawio-png-cli init
drawio-png-cli library add ./vendor/simple-icons.xml --title "Simple Icons"
drawio-png-cli library add ./assets/Logo.svg --name logos            # 画像ファイルは data のエントリーになる
drawio-png-cli library list
drawio-png-cli library search postgres --kind image
drawio-png-cli library show simple-icons/postgresql
drawio-png-cli library preview postgres -o candidates.png
```

- `library add` は CDATA・実体参照・圧縮のどの形式のライブラリも読み、VS Code 拡張と draw.io の両方が読める書式に変換して登録します。各エントリーに安定した ID を付け、タイトルが変わっても参照が変わらないようにします。
- draw.io Desktop などで登録ファイルを編集した場合は、`library sync` で書式を戻します。`library sync --check` は CI で崩れを検出します。
- ライブラリやアイコンのライセンス・商標の条件は、それぞれの提供元に従ってください。

### スコープと取得元

複数のリポジトリで使うライブラリは、スコープを付けて取得元から取り込みます。取得元は `.drawio-png/config.toml`（個人の取得元はユーザーの `config.toml`）にスコープごとに書きます。

```toml
[scopes]
acme = "git+https://github.com/acme/drawio-libraries.git"
partner = { url = "https://libs.example.com/drawio/", token-env = "PARTNER_LIBS_TOKEN" }
mine = "file:~/drawio-libraries"

[dependencies]
"@acme/brand" = "^1.0.0"
```

```sh
drawio-png-cli library install @acme/brand        # [dependencies] に追加し、lock.json に版を記録する
drawio-png-cli library install --frozen           # clone 後・CI で lock.json の版を復元する
drawio-png-cli library update @acme/brand         # 範囲内の新しい版にする

# 配布する側: 取得元のディレクトリに版を作り、git push などで公開する
drawio-png-cli library pack ./Logo.svg --name @acme/brand --version 1.0.0 --to ./drawio-libraries
```

- 取得元はディレクトリ（`file:`）・Git・HTTPS のいずれかで、中身は `@<スコープ>/<名前>/versions.json` と版ごとのライブラリファイルです。公開・認証は CLI では扱いません。
- 取り込んだファイルは `lock.json` の完全性（SHA-256）で照合します。ネットワークに接続するのは `library install`・`library update` だけです。
- XML の参照には版を書きません。保存済みの PNG は参照を含まないため、版を上げても既存の図面は変わりません。

詳細は [仕様の 9 章](docs/v0.2.0_claude/spec.md#9-ライブラリ) を参照してください。

### VS Code 拡張との共有

`library vscode --write` は、登録・取り込みしたライブラリを VS Code 拡張（`hediet.vscode-drawio`）の `customLibraries` として `.vscode/settings.json` に書きます。人は拡張のライブラリ欄から、エージェントは XML の参照から、同じ部品を使えます。

```json
{
  "hediet.vscode-drawio.customLibraries": [
    {
      "entryId": "drawio-png-cli",
      "libName": "Simple Icons",
      "file": "${workspaceFolder}/.drawio-png/libraries/simple-icons.xml"
    },
    {
      "entryId": "drawio-png-cli @acme",
      "libName": "ACME Brand",
      "file": "${workspaceFolder}/.drawio-png/libraries/@acme/brand.xml"
    }
  ]
}
```

拡張は CDATA や実体参照で書かれたライブラリを読めないため、CLI は登録時に書式を変換します。拡張の読み込みの制約と、人とエージェントの往復の流れは [VS Code 拡張との連携](docs/v0.2.0_claude/vscode.md) を参照してください。

## draw.io の組み込み図形（組み込みライブラリ）

AWS・Azure・Google Cloud（GCP2）・Kubernetes・UML など、draw.io のサイドバーにある図形（`shape=mxgraph.*` など）を、style を推測せずに参照できます。drawio-png-cli は、VS Code 拡張 1.9.0 と同じ draw.io 26.0.2 のサイドバーから作った図形の一覧（約 440 の図形集、約 12,700 図形）を同梱しています。各図形集は `@drawio/aws4-compute` のような名前の組み込みライブラリで、登録は要りません。

```sh
drawio-png-cli library search lambda --scope drawio     # 参照文字列・種類・サイズ・タイトル（style は出さない）
drawio-png-cli library show @drawio/aws4-compute/lambda # style・セル構成・配置例
drawio-png-cli library preview lambda --scope drawio -o shapes.png
```

- 旧版の図形集（`@drawio/aws3-*` など）も含みます。サイドバーにタイトルが無い図形（AWS のグループなど）は、ラベルや図形名をタイトルにしています。
- 一覧には、図形のセルと style だけが入っています。アイコンの画像データ（`data:image/...`）を含む図形（GCP2 の製品別アイコンなど）は収録していません。それらのアイコンはカスタムライブラリから参照してください。上流の資材に画像やステンシルが無く描画できない5図形も除いています。
- AWS などの draw.io 図形名（`mxgraph.*`）を使う図形の描画には、draw.io 26.0.2 の Web 資材（`DRAWIO_PNG_DRAWIO_WEB_PATH`）が必要です。
- 一覧は `python3 scripts/shape_catalog.py /path/to/drawio-checkout` で作り直せます（開発用。Chrome が必要）。

## レンダラーの選択

`write --renderer` で選びます。既定は VS Code 拡張と同じ見た目になる `vscode` です。

| 値 | 描画 |
| --- | --- |
| `vscode`（既定） | Chromium。VS Code 拡張 1.9.0（draw.io 26.0.2）互換。SVG→Canvas で PNG 化し、XML の `scale`・`border` を反映 |
| `desktop-compat` | Chromium。draw.io Desktop 互換。DPR 2 でキャプチャし、Hamming1 で半分へ縮小 |
| `raw` | Chromium。draw.io の描画結果を DPR 1 でキャプチャし、画素の縮小・再加工をせず使用 |
| `desktop-app` | draw.io Desktop 27.0.2 以降のアプリ |

```sh
drawio-png-cli write diagram.xml -o diagram.drawio.png                          # vscode
drawio-png-cli write diagram.xml -o desktop.drawio.png --renderer desktop-compat
drawio-png-cli write diagram.xml -o app.drawio.png --renderer desktop-app
```

優先順位は、コマンドのオプション → 環境変数 `DRAWIO_PNG_RENDERER` → プロジェクトの `config.toml` → ユーザーの `config.toml` → `vscode` です。プロジェクトで描画方式とフォントを揃える場合は `config.toml` に書きます。

```toml
[render]
renderer = "vscode"
default-font = "Noto Sans CJK JP"
fallback-fonts = ["Noto Sans CJK JP", "Noto Color Emoji"]
allow-network = false
```

指定したレンダラーが見つからない場合や描画に失敗した場合は、別のレンダラーに切り替えず（v0.1.0 の `auto` はありません）、既存出力を保持してエラー終了します。

全方式で指定ページを描画し、全ページの編集用 XML を PNG に埋め込みます。`raw` もメタデータなし PNG を生成する方式ではありません。`raw`・`desktop-compat` は倍率1・余白0で draw.io のエクスポーターを呼び、XML の `scale`・`border` は反映しません。`desktop-compat` は参照環境の処理に合わせたもので、異なるフォント・ブラウザー・draw.io 資材での完全一致は保証しません。

`vscode` は拡張の安定版1.9.0が同梱する draw.io 26.0.2 に合わせ、`raw`・`desktop-compat` は Desktop 31.4.5 に合わせた資材をそれぞれ同梱しています。拡張の保存結果は保存時のエディターの表示倍率で変わり、drawio-png-cli は倍率100%の結果に合わせます。devcontainer や WSL でも図はホスト側の VS Code が描画するため、Windows と Linux ではフォント等による画素差が残ります。詳細は [描画互換性](docs/rendering.md) を参照してください。

## フォントの統一

`--default-font` でセルの `fontFamily` が未指定の場合のフォントを選び、`--fallback-font` で代替候補を指定できます。どのレンダラーにも同じフォント候補を渡し、設定を全ページの XML にも保存します。`config.toml` の `[render]` の `default-font`・`fallback-fonts` でも指定でき、`expand` にも適用します。

```sh
drawio-png-cli write diagram.xml -o diagram.drawio.png \
  --default-font "Noto Sans CJK JP" \
  --fallback-font "Noto Sans CJK JP" \
  --fallback-font "Noto Color Emoji"
```

この例では、未指定セルの第一候補が Noto Sans CJK JP になります。`fontFamily=Helvetica` など既存の指定は第一候補として保持し、そのフォントが使えない文字には Noto Sans CJK JP、Noto Color Emoji の順で代替候補を渡します。同じ候補は重複追加しません。既存候補に `sans-serif` などの一般名がある場合は、その手前に代替候補を挿入します。

- 特定のフォントを製品の既定値として強制しません。描画環境にフォントを用意し、そのファミリー名を指定してください。フォントの同梱・自動取得・インストール確認は行いません。
- `--fallback-font` は複数回指定でき、`--default-font` が必要です。1回につき1ファミリーを指定します。未インストール・未対応文字の場合は次の候補へ進み、すべて使えない場合はブラウザーの通常の代替処理になります。
- 対象は図形・接続線のセルの `fontFamily` です。HTML ラベル内の明示的なフォント指定、`inherit` などの継承指定、`fontSource` による Web フォント指定は保持します。名前付きスタイルだけを指定したセルでは、今回の既定フォントがセルの `fontFamily` として優先されます。
- `--metadata-only` でも XML に設定を保存できます。
- 同じ候補リストでも、フォントのバージョン・OS の別名解決・ブラウザーの文字計測・縮小処理が異なると寸法や画素に差が残ります。

仕様の詳細は [フォント指定の仕様](docs/fonts.md) を参照してください。

## Chromium / Chrome の準備

`desktop-app` 以外のレンダラーと `library preview` は Chromium / Chrome で描画します。既定では Chrome を使用し、Chrome が見つからない場合だけ Chromium を使います。検索順は `DRAWIO_PNG_CHROME_PATH` → `CHROME_PATH` → PATH → OS 標準インストール先です。PATH ではまず Chrome（`google-chrome`、`google-chrome-stable`、`chrome`、Windows では `chrome.exe`）をすべてのディレクトリから探し、なければ Chromium（`chromium`、`chromium-browser`、Windows では `chromium.exe`）を探します。Linux の標準インストール先は `/usr/bin/google-chrome`、`/opt/google/chrome/chrome`、`/usr/bin/chromium` の順です。明示指定のパスが不正な場合は別候補へ切り替えません。

```sh
# Chromium を使う場合は明示する
export DRAWIO_PNG_CHROME_PATH=/usr/bin/chromium
drawio-png-cli write diagram.xml -o diagram.drawio.png
```

ヘッドレスで起動するため Xvfb は不要です。Node.js、Python、ChromeDriver、オンライン版 draw.io への接続も不要です。フォントは OS にインストールされたものを使用するので、日本語には日本語フォントを用意してください。

同梱資材は基本図形・接続線・日本語・HTML ラベル・埋め込み画像・埋め込みステンシル（`shape=stencil(...)`）に対応します。埋め込みステンシルは展開後の合計64 MiBまでとし、壊れたデータや、`include-shape` で参照する図形が見つからない場合はエラーにします。追加アイコン・ステンシル、数式、Mermaid、自動レイアウトの資材は含みません。未知の図形や必要資材の欠落はエラーになります。同梱資材で対応できない図面には、別途用意した draw.io Web 資材を指定してください。

```sh
# export3.html を含む Webアプリのルートを指定（拡張1.9.0と同じ draw.io 26.0.2 の例）
git clone https://github.com/jgraph/drawio.git
git -C drawio checkout 96a916a337d13fc8bf622c8a67d422bd284eabe5
export DRAWIO_PNG_DRAWIO_WEB_PATH="$PWD/drawio/src/main/webapp"
drawio-png-cli write diagram.xml -o diagram.drawio.png
```

`DRAWIO_PNG_DRAWIO_WEB_PATH` を解除すると同梱資材に戻ります。指定が不正な場合はエラーとし、同梱資材には切り替えません。互換性を確認した上流コミットは、`vscode` が `96a916a337d13fc8bf622c8a67d422bd284eabe5`（26.0.2、拡張1.9.0と同じ）、`raw`・`desktop-compat` が `f3abfe0f082c18f7b4fee8a34c2d07b1987687fd`（31.4.5）です。`vscode` では、拡張と同じく `js/shapes-14-6-5.min.js`・`js/stencils.min.js` があれば描画前に読み込みます。draw.io 26.0.2 には `shapes/` フォルダーがないため、AWS などの図形はこれらから読み込みます。別バージョンでは `export3.html` と、`vscode` では `Graph`・`Editor`・`Editor.exportToCanvas()`、`raw`・`desktop-compat` では `render()`・`LoadingComplete` の互換性が必要です。

図面が参照する外部画像・Webフォントの取得は既定で禁止し、必要な外部資材がある場合はエラーにします。明示的に許可するときは次を使います。

```sh
drawio-png-cli write diagram.xml -o diagram.drawio.png --allow-network
```

`--allow-network` は外部HTTP(S)画像・フォント等の資材取得を許可します。`vscode` では、外部画像・フォントのCanvasへの埋め込みに配信元のCORS許可も必要です。外部スクリプト、フレーム、任意のローカルファイルの読み込みは許可しません。Chrome 自体も、`--disable-background-networking` 等を指定しても Google のサービス（コンポーネント更新、最適化ガイド、プッシュ通知、アカウント等）へ接続するため、名前解決で止めています。`--allow-network` なしではループバック以外の名前解決をすべて失敗させ、環境変数やシステム設定のプロキシも使いません（プロキシは接続先の名前解決を代行するため）。`--allow-network` ありでは図面の資材を取得するため、既知のサービス用ホストだけを止め、プロキシ設定もそのまま使います。このとき `www.google.com`・`www.gstatic.com` への Chrome 自身の接続は残り、プロキシを使う環境ではサービス用ホストへの接続もプロキシ経由で行われ得ます。外部への通信を確実に避けるには `--allow-network` を指定しないでください。Web資材は、ページ本体をループバック限定の一時HTTPサーバーで、スクリプト等の資材をDevTools Protocol経由で直接ブラウザーに渡し、処理後にサーバーと専用ブラウザープロファイルを片付けます。

追加オプションは `DRAWIO_PNG_CHROME_ARGS` にPOSIX形式で指定します。

```sh
export DRAWIO_PNG_CHROME_ARGS='--disable-gpu --disable-dev-shm-usage'
```

シェル展開は行いません。追加オプションは `--name` または `--name=value` 形式とし、値に空白があれば引用符で囲みます。プロファイル、デバッグ接続、ネットワーク制御など drawio-png-cli が管理するオプションは指定できません。sandbox は自動で無効化しません。テスト用コンテナで必要な場合に限り `--no-sandbox` を明示できます。

描画しないコマンド（`read`・`expand`・`validate`・`write --metadata-only`、`preview` 以外の `library` の各コマンド）と `--renderer desktop-app` では、Chromium 用の環境変数を読みません。

## Desktop の準備

`--renderer desktop-app` は draw.io Desktop のアプリで描画します。draw.io Desktop の検索順は次のとおりです。

1. `DRAWIO_PNG_DRAWIO_PATH` に指定された実行ファイル
2. PATH 内の `drawio`／`draw.io`（Windows では `.exe`）
3. OS の標準インストール先（macOS の `/Applications` と `~/Applications`、Windows の `ProgramFiles`／`LOCALAPPDATA`、Linux の `/opt/drawio/drawio`）

```sh
export DRAWIO_PNG_DRAWIO_PATH=/Applications/draw.io.app/Contents/MacOS/draw.io
```

明示指定が不正な場合、別の実行ファイルには切り替えません。描画のタイムアウトは 60 秒です。Desktop が見つからない場合や描画に失敗した場合はエラー終了し、既存出力は変更しません。

Desktop の追加オプションは `DRAWIO_PNG_DRAWIO_ARGS` に指定します。`DRAWIO_PNG_DRAWIO_PATH` には実行ファイルのパスだけを指定してください。

```sh
export DRAWIO_PNG_DRAWIO_PATH=/opt/drawio/drawio
export DRAWIO_PNG_DRAWIO_ARGS='--disable-gpu --disable-dev-shm-usage'

# GUI のないコンテナでは、Xvfb 経由で drawio-png-cli を起動する
xvfb-run -a drawio-png-cli write diagram.xml -o diagram.drawio.png --renderer desktop-app
```

Desktop のほかに `xvfb`・`xauth`、日本語を描画する場合は日本語フォントをコンテナに用意してください。`DRAWIO_PNG_DRAWIO_ARGS` の指定だけでは仮想ディスプレイは起動しません。

空白を含む値は引用符で囲みます。

```sh
export DRAWIO_PNG_DRAWIO_ARGS='--disable-gpu --user-data-dir="/tmp/drawio profile"'
```

- 全 OS で POSIX シェル形式の引用符・バックスラッシュによる引数分割を使います。Windows のパスも引用符で囲むなど、この形式に合わせて指定してください。
- シェルは起動せず、環境変数・`~`・ワイルドカード・コマンド置換は展開しません。必要な値は明示的に指定します。
- 未設定・空文字・空白のみなら追加引数はありません。閉じていない引用符や Unicode として読めない値は、Desktop 起動前に終了コード `1` のエラーにします。
- 追加引数は `drawio-png-cli` が生成する描画引数の前に渡します。入力ファイル、出力先、形式、ページ選択は `drawio-png-cli` が管理するため、これらを変更するオプションや `--` は指定しないでください。
- `--renderer desktop-app` 以外では `DRAWIO_PNG_DRAWIO_ARGS` を読みません。

既存の Xvfb ラッパーを `DRAWIO_PNG_DRAWIO_PATH` に指定する方法も使えます。ラッパーでは `"$@"` を転送してください。

```sh
#!/bin/sh
exec xvfb-run -a /opt/drawio/drawio "$@"
```

`drawio-png-cli` は Electron の sandbox を自動で無効化しません。テスト用コンテナで sandbox を無効化する必要がある場合は、`DRAWIO_PNG_DRAWIO_ARGS` に `--no-sandbox` を明示的に追加できます。

## 検証・互換性・保存

- 入力 XML は UTF-8（BOM 可）。XML 構文、`mxfile`／`mxGraphModel` ルート、各ページの `root` 直下の基盤セル `id="0"`・`id="1"` を検証します。空の図面や壊れた圧縮ページは拒否します。
- 参照を展開した後、全ページについて ID の一意性、`parent`・`source`・`target` の参照先、`parent` の循環、参照が残っていないことを検査します。違反はすべて報告します。
- 構造検証は、すべての描画・レイアウト不具合を防ぐ保証ではありません。見た目は PNG を開いて確認してください。
- PNG はチャンクの CRC に加え、画像データ（IDAT）の zlib ストリームが終端し Adler-32 が一致することを検査します。ベース画像・レンダラー出力が壊れている場合は既存出力を変更せずエラーにします。
- `tEXt`／`zTXt` の `mxfile`／`mxGraphModel` を読み取り、旧 Desktop の raw DEFLATE、URL エンコードの二重化にも対応します。競合する複数の図面メタデータは拒否します。
- `read`・`expand` は全ページを非圧縮 XML にします。ページ順・名前・属性・未知要素を保持しますが、元の XML 文字列との完全一致は保証しません。
- 保存時は既存の図面メタデータを置換し、URL エンコードした XML を一つの `tEXt` チャンクへ格納します。ベース画像の画像データ・無関係なチャンクは保持します。
- 入力、展開データ、出力 PNG、デコード後の画像バッファに 64 MiB の上限があります。Chromium では取得画像にもこの制限を適用します。`raw` は最大16,777,216画素、`desktop-compat` は2倍取得のため最終画像が最大4,194,304画素です。`vscode` は中間SVG画像と最終Canvasがそれぞれ最大16,777,216画素・一辺16,384pxです。超過時は自動調整せずエラーにします。DTD と外部エンティティは受け付けません。
- 出力先と同じディレクトリの一時ファイルに保存・同期後、アトミックに置換します。既存ファイルの権限を引き継ぎ、失敗時の一時ファイルは片付けます。出力先ディレクトリは事前に作成してください。

## 環境変数

| 環境変数 | 内容 |
| --- | --- |
| `DRAWIO_PNG_PROJECT` | プロジェクト領域（`.drawio-png/`）を含むディレクトリ。既定はカレントディレクトリから親へ探す |
| `DRAWIO_PNG_HOME` | ユーザー領域（個人の `config.toml`）。既定は Linux が `$XDG_CONFIG_HOME/drawio-png-cli`、macOS が `~/Library/Application Support/drawio-png-cli`、Windows が `%APPDATA%\drawio-png-cli` |
| `DRAWIO_PNG_RENDERER` | 既定のレンダラー（`vscode`・`desktop-compat`・`raw`・`desktop-app`） |
| `DRAWIO_PNG_CHROME_PATH` / `CHROME_PATH` | Chromium / Chrome の実行ファイル |
| `DRAWIO_PNG_CHROME_ARGS` | Chromium の追加引数（POSIX 形式のクォート。シェル展開はしない） |
| `DRAWIO_PNG_DRAWIO_PATH` / `DRAWIO_PNG_DRAWIO_ARGS` | draw.io Desktop の実行ファイルと追加引数 |
| `DRAWIO_PNG_DRAWIO_WEB_PATH` | ローカルの draw.io Web 資材のディレクトリ（既定は同梱資材）。AWS などの図形の描画に必要 |
| `[scopes]` の `token-env` で指定した名前 | HTTPS の取得元に送るトークン |
| `DRAWIO_PNG_CACHE` | 取得元から取得したファイルのキャッシュ（既定は OS のキャッシュディレクトリ） |

v0.1.0 の `DRAWIO_PNG_LIBRARY_PATH`・`DRAWIO_PNG_CHROMIUM_MODE` は廃止しました。

## AI エージェント向けスキル

`skills/drawio-png/` は、Claude Code などの AI エージェントに drawio-png-cli の使い方を教えるスキルです。中身は案内だけで、手順の本文は drawio-png-cli に同梱しており、`drawio-png-cli skill` で出力します。手順は常にインストールされている drawio-png-cli の版と一致します。

```sh
# 対応するエージェントへまとめてインストール（Vercel Labs の skills CLI）
npx skills add szk302/drawio-png-cli
# または Claude Code へ手動で配置
mkdir -p ~/.claude/skills && cp -r skills/drawio-png ~/.claude/skills/
```

## 開発とテスト

```sh
mise exec -- cargo fmt --check
mise exec -- cargo clippy --locked --all-targets -- -D warnings
mise exec -- cargo test --locked
mise exec -- cargo build --release --locked
```

固定 fixture による互換テスト、CLI の往復編集、保存失敗時の保護、Unix のテスト用レンダラーによる引数・異常終了・タイムアウト検証を含みます。CI は Linux・macOS・Windows で実行します。

Desktop・Chromium の実機テストは通常のテストから除外しています。描画環境を用意して明示的に実行してください。

```sh
DRAWIO_PNG_TEST_DRAWIO_PATH=/path/to/drawio-or-wrapper \
  mise exec -- cargo test --test desktop -- --ignored --nocapture
```

```sh
DRAWIO_PNG_TEST_CHROME_PATH=/usr/bin/google-chrome \
  mise exec -- cargo test --test chromium -- --ignored --nocapture
# テスト環境に必要な追加引数は DRAWIO_PNG_TEST_CHROME_ARGS で指定
python3 scripts/vendor_drawio.py --check
```

Linux CIではChromium実機テストも実行します。資材ハッシュの確認に使うPythonは開発用で、ビルド・実行時には不要です。

実装の参考にしたコードと fixture の説明は [tests/fixtures/README.md](tests/fixtures/README.md)、要件は [docs/prd.md](docs/prd.md)、v0.2.0 の仕様は [docs/v0.2.0_claude/spec.md](docs/v0.2.0_claude/spec.md) を参照してください。`.tmp` の参考リポジトリはビルドに使用しません。

変更は作業ブランチで行い、Conventional Commits 形式でコミットします。

## ライセンス

本プロジェクトの独自コードは [MIT License](LICENSE) で公開します。Desktop互換の縮小処理はBSD-3-Clauseです。Cargoのライセンス表記は同梱描画資材と縮小処理を含めて `MIT AND Apache-2.0 AND Zlib AND BSD-3-Clause` としています。
これは異なるライセンスの構成物を含むパッケージの表記で、独自コードのMITライセンスを変更するものではありません。再配布する構成物ごとの条件に従ってください。
参考にした drawio-exporter と VS Code Draw.io Integration の参照範囲・ライセンスは
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)、Cargo依存の著作権表示・ライセンス全文は
[依存ライセンス一覧](assets/licenses/cargo-dependencies.txt) に記載しています。

本プロジェクトは非公式の独立したプロジェクトであり、draw.ioの提供元であるJGraph Holdings Ltdおよびdraw.io AGとの提携・承認関係はありません。

ソース配布には `LICENSE` と `THIRD_PARTY_NOTICES.md` を含めます。
ソース・バイナリとも、この2ファイルと `assets/drawio/README.md`・`assets/drawio/licenses/`・`assets/licenses/`（Cargo依存の生成済み一覧を含む）を保持してください。
同梱描画資材は各資材のライセンスに従います。対象と除外範囲は [資材一覧](assets/drawio/README.md) に記録しています。`drawio-png-cli licenses` で本体・Cargo依存・同梱資材の通知とライセンス全文を確認できます。通常のビルドにライセンス生成ツールは不要です。

依存を更新した際は、次の手順で一覧を再生成し、差分を確認して `Cargo.lock` とともにコミットしてください。
一覧は本パッケージの全featuresで有効になる依存をOSを絞らず収集し、開発用・ビルド用の依存も含めています。`Cargo.lock` にあっても無効な任意依存や常に偽の `cfg` にある依存は対象外です。`about.toml` の優先順で許諾されたライセンスを選択し、`AND` の条件はすべて保持します。

```sh
mise exec -- cargo install --locked --version 0.9.2 cargo-about
mise exec -- cargo install --locked --version 0.20.2 cargo-deny
mise exec -- cargo fetch --locked
mise exec -- python3 scripts/cargo_licenses.py
mise exec -- cargo deny --locked check licenses
```

生成にはネットワーク接続が必要です。crateにライセンス原本がない場合は `about.toml` に固定した上流リビジョンとハッシュで補います。原本が見つからず汎用ライセンス文に置換された場合は生成を失敗させます。
CIでは `scripts/cargo_licenses.py --check` で更新漏れを検出し、許可ライセンスを検査します。Linux・macOS・Windowsのビルド後に通知一式を同梱したバイナリアーカイブを作成します。手元での作成例:

```sh
mise exec -- cargo build --release --locked
python3 scripts/package_binary.py target/release/drawio-png-cli target/drawio-png-cli.tar.gz
# Windowsでは入力を target/release/drawio-png-cli.exe に、出力を .zip に変更
```

リリースは、`Cargo.toml` の `version` と一致するタグ（例: `v0.2.0`）をプッシュすると `.github/workflows/release.yml` が作成します。CI と同じ検査を通過した後、Linux（musl）・macOS・Windows のバイナリを通知一式とともにアーカイブし、`SHA256SUMS` とビルド来歴の証明（attestation）を付けて GitHub Release に公開します。`v0.2.0-rc.1` のように `-` を含むタグはプレリリースになります。Linux 版は musl を静的リンクするため、`assets/licenses/musl-1.2.5-COPYRIGHT.txt` を同梱しています。`mise.toml` の Rust を更新した際は、そのツールチェーンが使う musl の版（Rust リポジトリの `src/ci/docker/scripts/musl.sh`）を確認し、異なれば通知を差し替えてください。

```sh
git tag v0.2.0
git push origin v0.2.0
```
