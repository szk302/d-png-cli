# drawio-png-cli

`drawio-png-cli` は `.drawio.png` に埋め込まれた draw.io の XML を抽出・検証・更新する Rust 製 CLI です。圧縮ページを展開して AI で編集し、draw.io Desktop または Chromium / Chrome で PNG を再描画できます。

## インストール

[GitHub Releases](https://github.com/szk302/drawio-png-cli/releases) から、OS に合ったアーカイブを取得して展開し、`drawio-png-cli`（Windows は `drawio-png-cli.exe`）を PATH の通った場所に置きます。アーカイブにはライセンス通知一式を同梱しています。

| OS | アーカイブ |
| --- | --- |
| Linux x86_64 / ARM64 | `drawio-png-cli-<version>-x86_64-unknown-linux-musl.tar.gz` / `drawio-png-cli-<version>-aarch64-unknown-linux-musl.tar.gz`（静的リンク） |
| macOS Apple silicon / Intel | `drawio-png-cli-<version>-aarch64-apple-darwin.tar.gz` / `drawio-png-cli-<version>-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `drawio-png-cli-<version>-x86_64-pc-windows-msvc.zip` |

```sh
# 例: Linux x86_64。SHA256SUMS で改ざんを確認する
version=0.1.0
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

Rust を直接管理している場合は `cargo install --path . --locked` でもインストールできます。実行ファイル名は `drawio-png-cli` です。抽出・検証・メタデータ更新には Node.js や Python は不要です。画像の再描画には draw.io Desktop 27.0.2 以降、または Chromium / Chrome が必要です。Chromium 用の基本描画資材はバイナリに同梱しています。

## 使い方

```sh
# 圧縮ページを展開して、全ページの編集可能な XML を取得
drawio-png-cli extract diagram.drawio.png -o diagram.xml

# XML または PNG 内の図面を検証
drawio-png-cli validate diagram.xml
drawio-png-cli validate diagram.drawio.png

# XML から先頭ページを描画し、全ページの XML を PNG に保存（Desktop優先、なければChromium）
drawio-png-cli embed -i diagram.xml -o diagram.drawio.png

# XML は stdin でも指定可能
cat diagram.xml | drawio-png-cli embed -o diagram.drawio.png

# ベース画像の見た目を維持し、XML だけを更新（同じパスでも可）
drawio-png-cli embed -i diagram.xml --no-render -b diagram.drawio.png -o diagram.drawio.png

# ベース画像なしなら、透明な 1×1 PNG に XML を保存
drawio-png-cli embed -i diagram.xml --no-render -o new.drawio.png
```

`extract` の `-o` を省略すると XML を stdout に出力します。診断・警告は stderr に出力します。正常終了は `0`、処理・検証エラーは `1`、引数エラーは `2` です。描画中に SIGINT（Ctrl+C）・SIGTERM・SIGHUP（Windows では Ctrl+C・Ctrl+Break）を受けると、描画を中断してレンダラーのプロセスと一時ファイルを片付け、既存出力を変更せずにそのシグナルで終了します（シェルでの終了コードは `128 + シグナル番号`、SIGINT なら `130`）。描画以外の処理中は通常どおり直ちに終了し、`nohup` などで無視されているシグナルは無視したままです。SIGKILL などの強制終了では片付けられません。

`--base-image` は `--no-render` と組み合わせます。`--no-render` は画像と XML の見た目が同期されない旨を警告します。ベース画像を指定しない場合、既存の出力 PNG の画像は再利用しません。

## コマンド・オプション一覧

各コマンドの詳細は `drawio-png-cli <コマンド> --help` でも確認できます。

| コマンド | 内容 |
| --- | --- |
| `drawio-png-cli extract INPUT [-o OUTPUT]` | `.drawio.png` から全ページの非圧縮 XML を取り出す。`-o` を省くと stdout に出力 |
| `drawio-png-cli validate INPUT` | XML または `.drawio.png` の図面（全ページ）を検証する |
| `drawio-png-cli embed -o OUTPUT [-i INPUT] [オプション]` | XML を PNG に保存する。`--no-render` がなければ先頭ページを描画する。`-i` を省くと stdin から読む |
| `drawio-png-cli insert -o OUTPUT [-i INPUT] [オプション]` | ライブラリのエントリー（組み込み図形を含む）を、非圧縮 XML のページに挿入し、新しいセルの ID を出力する |
| `drawio-png-cli library list` / `ls` | ライブラリの名前・エントリー数・表示名を一覧する（利用者のライブラリの後に組み込みライブラリ） |
| `drawio-png-cli library show NAME` | ライブラリ 1 つの詳細（名前・表示名・ファイル・エントリー数）を表示する |
| `drawio-png-cli library search [QUERY]` | エントリーを検索し、ライブラリ・番号・タイトル・サイズを出力する（style や画像データは出さない） |
| `drawio-png-cli library style LIBRARY INDEX` | 単一セルのエントリーの style を、XML の属性値としてエスケープして 1 行で出力する（画像・複数セルのエントリーはエラー） |
| `drawio-png-cli library preview [QUERY] -o OUTPUT` | 該当するエントリーを、ライブラリ・番号・タイトル付きで並べた PNG を描画する（最大 60 件） |
| `drawio-png-cli skill [--full]` | AI エージェント向けの手順書を出力する。`--full` で全機能版 |
| `drawio-png-cli licenses` | drawio-png-cli・依存クレート・同梱資材のライセンスと通知を出力する |

`drawio-png-cli library` の各コマンドは、図面やライブラリを変更しません（`preview` は指定した PNG だけを書きます）。検索語はタイトルに含まれる文字列で、大文字小文字を区別しません。ライブラリは、`DRAWIO_PNG_LIBRARY_PATH` の利用者のライブラリと、drawio-png-cli に同梱した組み込みライブラリ（`drawio/*`）です。

### `drawio-png-cli embed`

| オプション | 内容 |
| --- | --- |
| `-i`, `--input FILE` | 入力 XML。省くと stdin から読む |
| `-o`, `--output FILE` | 出力 PNG（必須）。入力と同じパスも可 |
| `--no-render` | 描画せず、XML（メタデータ）だけを保存する |
| `-b`, `--base-image FILE` | この PNG の画素をそのまま使う。`--no-render` が必要 |
| `--renderer auto\|desktop\|chromium` | 描画に使うレンダラー。既定は `auto`（Desktop を優先し、なければ Chromium） |
| `--chromium-mode raw\|desktop\|vscode` | Chromium の出力方式。`DRAWIO_PNG_CHROMIUM_MODE` より優先。既定は `vscode` |
| `--allow-network` | Chromium で外部 HTTP(S) の画像・フォントの取得を許可する |
| `--default-font FAMILY` | `fontFamily` 未指定のセルのフォントを設定し、XML にも保存する |
| `--fallback-font FAMILY` | 代替フォントを優先順に追加する（複数指定可）。`--default-font` が必要 |
| `--no-validate` | デバッグ用。XML の検証を省く |

`--renderer`・`--chromium-mode`・`--allow-network` は `--no-render` と併用できません。`--default-font` は `--no-validate` と併用できません。

### `drawio-png-cli insert`

| オプション | 内容 |
| --- | --- |
| `-i`, `--input FILE` | 入力 XML。省くと stdin から読む |
| `-o`, `--output FILE` | 出力 XML（必須）。入力と同じパスも可 |
| `--name TITLE` | エントリーをタイトルで選ぶ（完全一致、なければ大文字小文字を区別しない一意の一致） |
| `--index N` | ライブラリの中の番号（1 から）で選ぶ。ライブラリが 1 つに決まるときのみ |
| `--library NAME` | 使うライブラリを名前で絞る。利用者のライブラリはファイル名から `.xml` を除いたもの、組み込みは `drawio/<名前>`（その名前の利用者のライブラリが無ければ `drawio/` は省略可） |
| `--library-file FILE` | `DRAWIO_PNG_LIBRARY_PATH` と組み込みライブラリの代わりに、このライブラリのファイルだけを使う |
| `--no-builtin` | 組み込みライブラリ（`drawio/*`）を使わない |
| `--page N` | 挿入先のページ（1 から、既定 1） |
| `--x X`・`--y Y` | 挿入する図形の左上の位置（既定 0。負の値も可） |
| `--width W`・`--height H` | 単一セルの図形のサイズ。片方だけなら縦横比を維持 |
| `--id ID` | 最上位のセルの ID。複数セルの図形の他のセルは `<ID>-<n>` |
| `--label TEXT` | 最上位のセルのラベル（最上位のセルが 1 つの図形のみ） |

エントリーは `--name` か `--index` のどちらかで選びます。`--library` を付けずに `--name` を使うと、`DRAWIO_PNG_LIBRARY_PATH` のライブラリにそのタイトルがあればそちらを、無ければ組み込みライブラリから選びます。`--library-file` は `--library`・`--no-builtin` と併用できません。

### `drawio-png-cli library`

| コマンド | オプション | 内容 |
| --- | --- | --- |
| `list` | `--library-file FILE` | `DRAWIO_PNG_LIBRARY_PATH` と組み込みライブラリの代わりに、このファイルだけを使う |
| `list` | `--no-builtin` | 組み込みライブラリを含めない |
| `show` | `--library-file FILE` | このファイルだけを使う |
| `search`・`preview` | `--library NAME` | 1 つのライブラリに絞る（`insert` と同じ名前の規則） |
| `search`・`preview` | `--library-file FILE` | このファイルだけを使う |
| `search`・`preview` | `--no-builtin` | 組み込みライブラリを含めない |
| `preview` | `-o`, `--output FILE` | 出力 PNG（必須）。`--library` を指定しない場合は `QUERY` が必須 |

`style` は、ライブラリ名（`--library` と同じ規則）と番号を位置引数で受け取ります。

### 環境変数

| 環境変数 | 内容 |
| --- | --- |
| `DRAWIO_PNG_DRAWIO_PATH` / `DRAWIO_PNG_DRAWIO_ARGS` | draw.io Desktop の実行ファイルと追加引数 |
| `DRAWIO_PNG_CHROME_PATH` / `CHROME_PATH` | Chromium / Chrome の実行ファイル |
| `DRAWIO_PNG_CHROME_ARGS` | Chromium の追加引数（POSIX 形式のクォート。シェル展開はしない） |
| `DRAWIO_PNG_CHROMIUM_MODE` | Chromium の出力方式（`raw`・`desktop`・`vscode`、既定 `vscode`） |
| `DRAWIO_PNG_DRAWIO_WEB_PATH` | ローカルの draw.io Web 資材のディレクトリ（既定は同梱資材）。AWS などの図形の描画に必要 |
| `DRAWIO_PNG_LIBRARY_PATH` | ライブラリのファイルまたは `*.xml` を含むディレクトリ。`PATH` と同じ区切りで複数指定可 |

## AI エージェント向けスキル

`skills/drawio-png/` は、Claude Code などの AI エージェントに drawio-png-cli の使い方を教えるスキルです。中身は案内だけで、手順の本文は drawio-png-cli に同梱しており、`drawio-png-cli skill`（基本の手順、vscode モード）と `drawio-png-cli skill --full`（全機能）で出力します。手順は常にインストールされている drawio-png-cli の版と一致します。

```sh
# 対応するエージェントへまとめてインストール（Vercel Labs の skills CLI）
npx skills add szk302/drawio-png-cli
# または Claude Code へ手動で配置
mkdir -p ~/.claude/skills && cp -r skills/drawio-png ~/.claude/skills/
```

## カスタムライブラリの図形を挿入

draw.io のカスタムライブラリ（`<mxlibrary>` 形式の XML。Desktop の「File → Open Library」や VS Code 拡張の `hediet.vscode-drawio.customLibraries` で読み込むファイル）の図形・アイコン（エントリー）を、非圧縮の XML に挿入できます。drawio-png-cli は利用者のライブラリを同梱・取得しません。ライブラリのファイルは利用者の環境で用意し、`DRAWIO_PNG_LIBRARY_PATH` で指定します（draw.io のサイドバーの図形は、後述の組み込みライブラリとして同梱しています）。

`DRAWIO_PNG_LIBRARY_PATH` には、ファイルまたはディレクトリを `PATH` と同じ区切り（Linux・macOS は `:`、Windows は `;`）で指定します。ディレクトリは直下の `*.xml` を名前順に読み、読めないファイルは警告を出して読み飛ばします。ライブラリ名はファイル名から `.xml` を除いたものです。`--library-file` を指定すると、`DRAWIO_PNG_LIBRARY_PATH` と組み込みライブラリの代わりにそのファイルだけを使います。

```sh
export DRAWIO_PNG_LIBRARY_PATH="$HOME/.cache/drawio-libraries"

# ライブラリの一覧（ライブラリ名・エントリー数・表示名）。ls でも可
drawio-png-cli library list
#   simple-icons	3463	Simple Icons

# 1つのライブラリの詳細
drawio-png-cli library show simple-icons

# エントリーの検索（ライブラリ名・番号・タイトル・サイズ。画像データは出力しない）
drawio-png-cli library search postgres
#   simple-icons	2332	PostgreSQL	144x144
drawio-png-cli library search --library simple-icons   # 1つのライブラリの全エントリー

# 図面への挿入（挿入したセルの ID を出力。入力と同じパスに保存可）
drawio-png-cli extract diagram.drawio.png -o diagram.xml
drawio-png-cli insert --name PostgreSQL -i diagram.xml -o diagram.xml --id db --x 40 --y 40 --width 48 --label "DB"
drawio-png-cli embed -i diagram.xml -o diagram.drawio.png
```

- `drawio-png-cli library` の各コマンドはライブラリを調べるだけで、ファイルを変更しません。図面を変更するのは `drawio-png-cli insert` です。
- 検索語はタイトルに含まれる文字列で、大文字小文字を区別しません。省略すると全エントリーを出力します。
- `--name` はタイトルの完全一致、なければ大文字小文字を区別しない一意の一致で選びます。複数のライブラリにある場合は `--library` を、同じライブラリに同名が複数ある場合は `search` の番号を `--index`（ライブラリが1つに決まるときのみ）で指定します。
- 挿入する図形は、左上が `--x`・`--y` になるよう移動します。セルの ID は挿入先のページで重ならないよう振り直し、`--id` を指定するとそれを使います（複数セルの図形の他のセルは `<ID>-<n>`）。接続線の接続先・グループの親子関係も対応付けます。
- `--width`・`--height`（片方なら縦横比を維持）は1セルの図形、`--label` は最上位のセルが1つの図形に使えます。ラベルの位置・書式はライブラリの `style` のままです。
- 挿入先は `--page`（1から）の最初のレイヤーです。圧縮ページは展開し、挿入後の XML を検証してから保存します。
- 画像を `data:` URL で埋め込んだ図形（Simple Icons 等）は同梱資材のまま描画できます。ライブラリが外部 URL の画像を参照する場合、描画には `--allow-network` が必要です。AWS 等の draw.io 図形名（`mxgraph.*`）を使う図形には、描画時に `DRAWIO_PNG_DRAWIO_WEB_PATH` が必要です。
- ライブラリやアイコンのライセンス・商標の条件は、それぞれの提供元に従ってください。

## draw.io の組み込み図形（組み込みライブラリ）

AWS・Azure・Google Cloud（GCP2）・Kubernetes・UML など、draw.io のサイドバーにある図形（`shape=mxgraph.*` など）の style を、推測せずに取り出したり挿入したりできます。drawio-png-cli は、VS Code 拡張 1.9.0 と同じ draw.io 26.0.2 のサイドバーから作った図形の一覧（約 440 の図形集、約 12,700 図形）を同梱しています。各図形集は、`drawio/aws4-compute` のような名前の組み込みライブラリとして、利用者のライブラリと同じ `drawio-png-cli library` の各コマンドと `drawio-png-cli insert` で使えます。

```sh
drawio-png-cli library search lambda                  # ライブラリ・番号・タイトル・サイズ（style は出さない）
drawio-png-cli library style drawio/aws4-compute 16   # 1つの図形の style を1行で出力（XML を書くときに写す）
drawio-png-cli library preview lambda -o shapes.png   # 該当する図形を「ライブラリ #番号」とタイトル付きで並べた PNG
drawio-png-cli insert --library drawio/aws4-compute --index 16 -i diagram.xml -o diagram.xml --id fn --x 40 --y 40
drawio-png-cli insert --library aws4-compute --name Lambda -i diagram.xml -o diagram.xml
```

- 組み込みライブラリは、`DRAWIO_PNG_LIBRARY_PATH` の利用者のライブラリの後に並びます。`DRAWIO_PNG_LIBRARY_PATH` が未設定でも使えます。`--no-builtin` で除外できます。
- `--library` では `drawio/` を省略できます。同じ名前の利用者のライブラリがある場合は、名前どおり利用者のライブラリを使います（組み込みは `drawio/` を付けて指定します）。
- 旧版の図形集（`drawio/aws3-*` など）も含みます。サイドバーにタイトルが無い図形（AWS のグループなど）は、ラベルや図形名をタイトルにしています。
- `style` は単一セルの図形だけに使えます。カードやグループなど複数セルの図形、画像のエントリーは `insert` で挿入します。
- 図形の描画には、従来どおり draw.io 26.0.2 の Web 資材（`DRAWIO_PNG_DRAWIO_WEB_PATH`）が必要です。`preview` も Chromium の vscode モードで描画し、一度に 60 件までです。
- 一覧には、図形のセルと style だけが入っています。アイコンの画像データ（`data:image/...`）を含む図形（GCP2 の製品別アイコンなど）は収録していません。それらのアイコンはカスタムライブラリから挿入してください。上流の資材に画像やステンシルが無く描画できない5図形も除いています。
- 一覧は `python3 scripts/shape_catalog.py /path/to/drawio-checkout` で作り直せます（開発用。Chrome が必要）。

## レンダラーの選択

`embed --renderer auto|desktop|chromium` で選択します。既定の `auto` は Desktop を優先し、見つからない場合だけ Chromium に切り替えます。明示パスが不正な場合や描画に失敗した場合は切り替えず、既存出力を保持してエラー終了します。

Chromiumの出力方式は `--chromium-mode raw|desktop|vscode` または環境変数 `DRAWIO_PNG_CHROMIUM_MODE` で選択します。優先順位はCLI指定 → 環境変数 → `vscode` です。

| モード | 出力方式 |
| --- | --- |
| `raw` | draw.ioの描画結果をDPR 1でキャプチャし、画素の縮小・再加工をせず使用 |
| `desktop` | Desktop互換。DPR 2でキャプチャし、Hamming1で半分へ縮小 |
| `vscode` | VS Code拡張1.9.0（draw.io 26.0.2）互換。SVG→CanvasでPNG化し、XMLの `scale`・`border` を反映 |

全モードで先頭ページを描画し、全ページの編集用XMLをPNGに埋め込みます。`raw` もメタデータなしPNGを生成するモードではありません。`raw`・`desktop` は倍率1・余白0でdraw.ioのエクスポーターを呼び、XMLの `scale`・`border` は反映しません。

```sh
drawio-png-cli embed --renderer chromium --chromium-mode raw -i diagram.xml -o raw.drawio.png
drawio-png-cli embed --renderer chromium --chromium-mode desktop -i diagram.xml -o desktop.drawio.png
drawio-png-cli embed --renderer chromium --chromium-mode vscode -i diagram.xml -o vscode.drawio.png
```

毎回同じモードを使う場合は、環境変数で指定できます。

```sh
export DRAWIO_PNG_CHROMIUM_MODE=desktop
drawio-png-cli embed --renderer chromium -i diagram.xml -o diagram.drawio.png
# 今回だけVS Code互換で出力
drawio-png-cli embed --renderer chromium --chromium-mode vscode -i diagram.xml -o diagram.drawio.png
```

環境変数の値は `raw`・`desktop`・`vscode` の完全一致で指定します。空文字・未知の値は描画前にエラーになります。CLIでモードを指定した場合は環境変数を読みません。Desktop選択時と `extract`・`validate`・`embed --no-render` でも読みません。環境変数はレンダラー選択を変更しないため、Chromiumを確実に使うには `--renderer chromium` を指定してください。

`--renderer desktop` はDesktopアプリを起動します。`--renderer chromium --chromium-mode desktop` はChromiumだけでDesktop互換処理を行います。互換モードは参照環境の処理に合わせたもので、異なるフォント・ブラウザー・draw.io資材・拡張設定での完全一致は保証しません。

`vscode` は拡張の安定版1.9.0が同梱するdraw.io 26.0.2に合わせ、`raw`・`desktop` はDesktop 31.4.5に合わせた資材をそれぞれ同梱しています。拡張の保存結果は保存時のエディターの表示倍率で変わり、drawio-png-cliは倍率100%の結果に合わせます。devcontainerやWSLでも図はホスト側のVS Codeが描画するため、WindowsとLinuxではフォント等による画素差が残ります。詳細は [描画互換性](docs/rendering.md) を参照してください。

`--renderer`・`--chromium-mode`・`--allow-network` は `--no-render` と併用できません。`--chromium-mode` と `--allow-network` はChromium専用です。`auto` でDesktopが選ばれる場合はエラーになるため、`--renderer chromium` も指定してください。

## フォントの統一

`--default-font` でセルの `fontFamily` が未指定の場合のフォントを選び、`--fallback-font` で代替候補を指定できます。Desktop・Chromiumに同じフォント候補を渡し、設定を全ページのXMLにも保存します。オプションを省略すると従来の指定を使用します。

```sh
drawio-png-cli embed --renderer chromium -i diagram.xml -o diagram.drawio.png \
  --default-font "Noto Sans CJK JP" \
  --fallback-font "Noto Sans CJK JP" \
  --fallback-font "Noto Color Emoji"
```

この例では、未指定セルの第一候補がNoto Sans CJK JPになります。`fontFamily=Helvetica` など既存の指定は第一候補として保持し、そのフォントが使えない文字にはNoto Sans CJK JP、Noto Color Emojiの順で代替候補を渡します。同じ候補は重複追加しません。既存候補に `sans-serif` などの一般名がある場合は、その手前に代替候補を挿入します。

- 特定のフォントを製品の既定値として強制しません。両方の描画環境に同じフォントを用意し、そのファミリー名を指定してください。フォントの同梱・自動取得・インストール確認は行いません。
- `--fallback-font` は複数回指定でき、`--default-font` が必要です。1回につき1ファミリーを指定します。未インストール・未対応文字の場合は次の候補へ進み、すべて使えない場合はブラウザーの通常の代替処理になります。
- 対象は図形・接続線のセルの `fontFamily` です。HTMLラベル内の明示的なフォント指定、`inherit` などの継承指定、`fontSource` によるWebフォント指定は保持します。名前付きスタイルだけを指定したセルでは、今回の既定フォントがセルの `fontFamily` として優先されます。
- `--no-render` でもXMLに設定を保存できます。`--no-validate` とは併用できません。
- 同じ候補リストでも、フォントのバージョン・OSの別名解決・ブラウザーの文字計測・縮小処理が異なると寸法や画素に差が残ります。

仕様の詳細は [フォント指定の仕様](docs/fonts.md) を参照してください。

## Chromium / Chrome の準備

既定では Chrome を使用し、Chrome が見つからない場合だけ Chromium を使います。検索順は `DRAWIO_PNG_CHROME_PATH` → `CHROME_PATH` → PATH → OS 標準インストール先です。PATH ではまず Chrome（`google-chrome`、`google-chrome-stable`、`chrome`、Windows では `chrome.exe`）をすべてのディレクトリから探し、なければ Chromium（`chromium`、`chromium-browser`、Windows では `chromium.exe`）を探します。Linux の標準インストール先は `/usr/bin/google-chrome`、`/opt/google/chrome/chrome`、`/usr/bin/chromium` の順です。明示指定のパスが不正な場合は別候補へ切り替えません。

```sh
# Chromium を使う場合は明示する
export DRAWIO_PNG_CHROME_PATH=/usr/bin/chromium
drawio-png-cli embed --renderer chromium -i diagram.xml -o diagram.drawio.png
```

ヘッドレスで起動するため Xvfb は不要です。Node.js、Python、ChromeDriver、オンライン版 draw.io への接続も不要です。フォントはOSにインストールされたものを使用するので、日本語には日本語フォントを用意してください。

同梱資材は基本図形・接続線・日本語・HTMLラベル・埋め込み画像・埋め込みステンシル（`shape=stencil(...)`）に対応します。埋め込みステンシルは展開後の合計64 MiBまでとし、壊れたデータや、`include-shape` で参照する図形が見つからない場合はエラーにします。追加アイコン・ステンシル、数式、Mermaid、自動レイアウトの資材は含みません。未知の図形や必要資材の欠落はエラーになります。同梱資材で対応できない図面には、Desktop または別途用意した draw.io Web資材を指定してください。

```sh
# export3.html を含む Webアプリのルートを指定（拡張1.9.0と同じ draw.io 26.0.2 の例）
git clone https://github.com/jgraph/drawio.git
git -C drawio checkout 96a916a337d13fc8bf622c8a67d422bd284eabe5
export DRAWIO_PNG_DRAWIO_WEB_PATH="$PWD/drawio/src/main/webapp"
drawio-png-cli embed --renderer chromium -i diagram.xml -o diagram.drawio.png
```

`DRAWIO_PNG_DRAWIO_WEB_PATH` を解除すると同梱資材に戻ります。指定が不正な場合はエラーとし、同梱資材には切り替えません。互換性を確認した上流コミットは、`vscode` が `96a916a337d13fc8bf622c8a67d422bd284eabe5`（26.0.2、拡張1.9.0と同じ）、`raw`・`desktop` が `f3abfe0f082c18f7b4fee8a34c2d07b1987687fd`（31.4.5）です。`vscode` では、拡張と同じく `js/shapes-14-6-5.min.js`・`js/stencils.min.js` があれば描画前に読み込みます。draw.io 26.0.2 には `shapes/` フォルダーがないため、AWS などの図形はこれらから読み込みます。別バージョンでは `export3.html` と、`vscode` では `Graph`・`Editor`・`Editor.exportToCanvas()`、`raw`・`desktop` では `render()`・`LoadingComplete` の互換性が必要です。

図面が参照する外部画像・Webフォントの取得は既定で禁止し、必要な外部資材がある場合はエラーにします。明示的に許可するときは次を使います。

```sh
drawio-png-cli embed --renderer chromium --allow-network -i diagram.xml -o diagram.drawio.png
```

`--allow-network` は外部HTTP(S)画像・フォント等の資材取得を許可します。`vscode` モードでは、外部画像・フォントのCanvasへの埋め込みに配信元のCORS許可も必要です。外部スクリプト、フレーム、任意のローカルファイルの読み込みは許可しません。Chrome 自体も、`--disable-background-networking` 等を指定しても Google のサービス（コンポーネント更新、最適化ガイド、プッシュ通知、アカウント等）へ接続するため、名前解決で止めています。`--allow-network` なしではループバック以外の名前解決をすべて失敗させ、環境変数やシステム設定のプロキシも使いません（プロキシは接続先の名前解決を代行するため）。`--allow-network` ありでは図面の資材を取得するため、既知のサービス用ホストだけを止め、プロキシ設定もそのまま使います。このとき `www.google.com`・`www.gstatic.com` への Chrome 自身の接続は残り、プロキシを使う環境ではサービス用ホストへの接続もプロキシ経由で行われ得ます。外部への通信を確実に避けるには `--allow-network` を指定しないでください。Web資材は、ページ本体をループバック限定の一時HTTPサーバーで、スクリプト等の資材をDevTools Protocol経由で直接ブラウザーに渡し、処理後にサーバーと専用ブラウザープロファイルを片付けます。

追加オプションは `DRAWIO_PNG_CHROME_ARGS` にPOSIX形式で指定します。

```sh
export DRAWIO_PNG_CHROME_ARGS='--disable-gpu --disable-dev-shm-usage'
```

シェル展開は行いません。追加オプションは `--name` または `--name=value` 形式とし、値に空白があれば引用符で囲みます。プロファイル、デバッグ接続、ネットワーク制御など drawio-png-cli が管理するオプションは指定できません。sandbox は自動で無効化しません。テスト用コンテナで必要な場合に限り `--no-sandbox` を明示できます。

`extract`・`validate`・`embed --no-render` はChromium用の環境変数を読みません。Desktop選択時もChromium用の環境変数は読みません。

## Desktop の準備

draw.io Desktop の検索順は次のとおりです。

1. `DRAWIO_PNG_DRAWIO_PATH` に指定された実行ファイル
2. PATH 内の `drawio`／`draw.io`（Windows では `.exe`）
3. OS の標準インストール先（macOS の `/Applications` と `~/Applications`、Windows の `ProgramFiles`／`LOCALAPPDATA`、Linux の `/opt/drawio/drawio`）

```sh
export DRAWIO_PNG_DRAWIO_PATH=/Applications/draw.io.app/Contents/MacOS/draw.io
```

明示指定が不正な場合、別の実行ファイルには切り替えません。描画のタイムアウトは 60 秒です。`--renderer desktop` で Desktop が見つからない場合や描画に失敗した場合はエラー終了し、既存出力は変更しません。

Desktop の追加オプションは `DRAWIO_PNG_DRAWIO_ARGS` に指定します。`DRAWIO_PNG_DRAWIO_PATH` には実行ファイルのパスだけを指定してください。

```sh
export DRAWIO_PNG_DRAWIO_PATH=/opt/drawio/drawio
export DRAWIO_PNG_DRAWIO_ARGS='--disable-gpu --disable-dev-shm-usage'

# GUI のないコンテナでは、Xvfb 経由で drawio-png-cli を起動する
xvfb-run -a drawio-png-cli embed -i diagram.xml -o diagram.drawio.png
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
- `extract`・`validate`・`embed --no-render` は `DRAWIO_PNG_DRAWIO_ARGS` を読みません。

既存の Xvfb ラッパーを `DRAWIO_PNG_DRAWIO_PATH` に指定する方法も使えます。ラッパーでは `"$@"` を転送してください。

```sh
#!/bin/sh
exec xvfb-run -a /opt/drawio/drawio "$@"
```

`drawio-png-cli` は Electron の sandbox を自動で無効化しません。テスト用コンテナで sandbox を無効化する必要がある場合は、`DRAWIO_PNG_DRAWIO_ARGS` に `--no-sandbox` を明示的に追加できます。

## 検証・互換性・保存

- 入力 XML は UTF-8（BOM 可）。XML 構文、`mxfile`／`mxGraphModel` ルート、各ページの `root` 直下の基盤セル `id="0"`・`id="1"` を検証します。空の図面や壊れた圧縮ページは拒否します。
- 構造検証は、すべての描画・レイアウト不具合を防ぐ保証ではありません。
- PNG はチャンクの CRC に加え、画像データ（IDAT）の zlib ストリームが終端し Adler-32 が一致することを検査します。ベース画像・レンダラー出力が壊れている場合は既存出力を変更せずエラーにします。
- `tEXt`／`zTXt` の `mxfile`／`mxGraphModel` を読み取り、旧 Desktop の raw DEFLATE、URL エンコードの二重化にも対応します。競合する複数の図面メタデータは拒否します。
- 抽出時は全ページを非圧縮 XML にします。ページ順・名前・属性・未知要素を保持しますが、元の XML 文字列との完全一致は保証しません。
- 保存時は既存の図面メタデータを置換し、URL エンコードした XML を一つの `tEXt` チャンクへ格納します。ベース画像の画像データ・無関係なチャンクは保持します。
- 入力、展開データ、出力 PNG、デコード後の画像バッファに 64 MiB の上限があります。Chromiumでは取得画像にもこの制限を適用します。`raw` は最大16,777,216画素、`desktop` は2倍取得のため最終画像が最大4,194,304画素です。`vscode` は中間SVG画像と最終Canvasがそれぞれ最大16,777,216画素・一辺16,384pxです。超過時は自動調整せずエラーにします。DTD と外部エンティティは受け付けません。
- 出力先と同じディレクトリの一時ファイルに保存・同期後、アトミックに置換します。既存ファイルの権限を引き継ぎ、失敗時の一時ファイルは片付けます。出力先ディレクトリは事前に作成してください。
- `--no-validate` はデバッグ用です。XML の構造検証・正規化を省略し、不正な XML も埋め込めます。UTF-8・サイズ制限・PNG 検査・アトミック保存は維持します。

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

実装の参考にしたコードと fixture の説明は [tests/fixtures/README.md](tests/fixtures/README.md)、要件は [docs/prd.md](docs/prd.md) を参照してください。`.tmp` の参考リポジトリはビルドに使用しません。

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

リリースは、`Cargo.toml` の `version` と一致するタグ（例: `v0.1.0`）をプッシュすると `.github/workflows/release.yml` が作成します。CI と同じ検査を通過した後、Linux（musl）・macOS・Windows のバイナリを通知一式とともにアーカイブし、`SHA256SUMS` とビルド来歴の証明（attestation）を付けて GitHub Release に公開します。`v0.2.0-rc.1` のように `-` を含むタグはプレリリースになります。Linux 版は musl を静的リンクするため、`assets/licenses/musl-1.2.5-COPYRIGHT.txt` を同梱しています。`mise.toml` の Rust を更新した際は、そのツールチェーンが使う musl の版（Rust リポジトリの `src/ci/docker/scripts/musl.sh`）を確認し、異なれば通知を差し替えてください。

```sh
git tag v0.1.0
git push origin v0.1.0
```
