# VS Code 拡張との連携

v0.2.0 では、CLI に登録したライブラリを VS Code の draw.io 拡張でもそのまま使えるようにする。人が VS Code で図面を編集するときと、エージェントが CLI で図面を作るときに、同じ部品を同じ見た目で使えることが目的である。コマンドの仕様は [spec.md](spec.md) を参照。

## 1. 対象

- 拡張: [`hediet.vscode-drawio`](https://github.com/hediet/vscode-drawio)（Draw.io Integration）安定版 1.9.0（タグ `v1.9.0`、コミット `132921f1c189d1b6239e2b0246ebd8190ca23ba5`）。同梱の draw.io は 26.0.2（`96a916a337d13fc8bf622c8a67d422bd284eabe5`）。`write --renderer vscode` はこの版に合わせている。
- 以下の拡張の動作は、ローカルの拡張・draw.io のソースを読んで確認したものである。VS Code 上で拡張を操作した確認は含まない。ライブラリ書式の読み込みは、拡張と同じパーサー（`xml-parser-xo` 3.1.0）と同じ手順で試した（[3 章](#3-ライブラリファイルの書式の制約)）。

## 2. 役割の分担

| 作業 | 担当 |
| --- | --- |
| ライブラリの登録・正規化・ID の付与 | CLI（`library add`・`library sync`） |
| 共有ライブラリの取り込み・版の固定 | CLI（`library install`・`library update`） |
| 拡張の設定（`customLibraries`）の生成 | CLI（`library vscode --write`） |
| XML からの PNG 作成・参照の展開・検証 | CLI（`write`・`validate`） |
| 図面の目視での編集・微調整 | 人（VS Code 拡張） |
| 登録ライブラリの部品の配置 | 人は拡張のライブラリ欄から、エージェントは XML の `libref`・`image=lib:` で |

`write` が保存する PNG の XML は参照を含まない普通の draw.io XML なので、拡張で開くために CLI は不要である。

## 3. ライブラリファイルの書式の制約

拡張 1.9.0 は、設定の `file` が `.json` で終わればファイル全体を JSON として、それ以外は XML として読む（`src/Config.ts` の `customLibraries`）。XML の場合の処理は次のとおり。

```ts
const parsedXml = parse(xml);                          // xml-parser-xo
return JSON.parse(parsedXml.root.children[0].content); // 最初の子のテキストをそのまま JSON に
```

`xml-parser-xo` 3.1.0 は、テキストを `[^<]+` で切り出すだけで実体参照（`&lt;`・`&amp;` など）を解かない。CDATA は `<![CDATA[` を含めた文字列を返し、中身に `]` や `>` があると CDATA として認識しない。このため、draw.io では読めるライブラリでも拡張では読めない場合がある。

拡張と同じパーサーと手順で、この仕様の例のファイルを読んだ結果:

| ファイル | 形式 | 結果 |
| --- | --- | --- |
| `examples/sources/components.xml` | CDATA で JSON を包む（v0.1.0 の文書の例と同じ形） | 失敗（`children[0]` がない） |
| `examples/sources/simple-icons.xml` | `<mxlibrary>` 直下に JSON、`xml` は圧縮 | 成功 |
| `examples/project/.d-png/libraries/*.xml` | [spec.md 9.3](spec.md#93-登録ファイルの書式) の書式（非圧縮の `xml` を含む） | 成功（非圧縮の `xml` も先頭が `<` の文字列として得られる）。`@acme/brand` の版のファイルは、このとき確かめた `brand-images.xml` と表示名だけが違う |
| 実際の Simple Icons for draw.io 16.35.0（3,466 エントリー） | `<mxlibrary>` 直下に JSON、`xml` は圧縮 | 成功 |

実体参照を使ったファイルは、パーサーのソースから次のように読まれると判断した（実行はしていない）: JSON の外側に `&quot;` があれば `JSON.parse` が失敗し、文字列の中の `&lt;` はそのまま残るため、非圧縮の `xml` が `&lt;mxGraphModel…` となって図形を作れない。draw.io 31.4.6 のソースでは、既定設定でライブラリを保存すると `xml` を非圧縮にし、DOM の直列化で `<` を `&lt;` にする（`EditorUi.createLibraryDataFromImages`）。draw.io Desktop でライブラリを編集・保存した後は、`library sync` で書式を戻す。

CLI の書式（spec.md 9.3）は、JSON の `\u003c` などで `<`・`>`・`&` を表し、XML としての特殊文字を一つも含まない。draw.io（DOM で解析）と拡張（テキストをそのまま使う）のどちらでも同じ文字列になる。`.json` 形式は draw.io Desktop の「ライブラリを開く」で読めないため使わない。

## 4. 拡張の設定を作る

```sh
d-png-cli init
d-png-cli library add ./vendor/simple-icons.xml --title "Simple Icons"
d-png-cli library add ./design/components.xml --title "Components"
d-png-cli library install @acme/brand        # config.toml の [scopes] に acme の取得元を書いておく
d-png-cli library vscode --write
```

`.vscode/settings.json` に次の項目が書かれる（[例のファイル](examples/project/.vscode/settings.json)）。

```json
{
  "hediet.vscode-drawio.customLibraries": [
    {
      "entryId": "d-png-cli",
      "libName": "Components",
      "file": "${workspaceFolder}/.d-png/libraries/components.xml"
    },
    {
      "entryId": "d-png-cli",
      "libName": "Simple Icons",
      "file": "${workspaceFolder}/.d-png/libraries/simple-icons.xml"
    },
    {
      "entryId": "d-png-cli @acme",
      "libName": "ACME Brand",
      "file": "${workspaceFolder}/.d-png/libraries/@acme/brand.xml"
    }
  ]
}
```

- 拡張の `file` は絶対パスで、`${workspaceFolder}` を使える。CLI はプロジェクト領域の親ディレクトリをワークスペースのフォルダーとみなして相対パスを作る。
- 拡張は同じ `entryId` のライブラリを 1 つの見出しにまとめ、見出しごとに表示・非表示を切り替えられる（`src/DrawioClient/simpleDrawioLibrary.ts`）。CLI はプロジェクト専用のライブラリを `d-png-cli`、取り込んだライブラリをスコープごとに `d-png-cli @acme` の見出しにする。名前空間がそのまま拡張の見出しになる。
- `--entry-id` で `d-png-cli` の部分を変えられる。`--write` はこの見出しの項目だけを置き換え、利用者が手で追加した他の項目は残す。
- 取り込んだライブラリのファイルは `.gitignore` で除外される。clone した直後は、`library install` を実行するまで拡張は `@acme` の見出しのライブラリを読めない（[7 章](#7-注意と未検証の事項)）。

### プロジェクト専用と取り込みの選び方

| 観点 | プロジェクト専用（`components`） | 取り込み（`@acme/brand`） |
| --- | --- | --- |
| 向いている部品 | そのリポジトリだけで使う部品 | 複数のリポジトリで使う部品（社内のロゴ・部品集、個人のアイコン集） |
| Git での扱い | ライブラリのファイルをコミットする（Simple Icons は約 5.6 MB） | `config.toml`・`lock.json` だけをコミットし、ファイルは `library install` で復元する |
| 更新 | `library add --merge`・`library sync` | 取得元で `library pack` した版を `library update` で取り込む |
| VS Code | clone 直後から読める | `library install` の後で読める |
| CI での `write`・`validate` | 準備は要らない | `library install --frozen` が要る（取得元への接続と認証） |

どちらも参照はプロジェクトの中のファイルだけで解決するため、同じコミットの XML はどこで実行しても同じ部品になる。個人のライブラリを複数のリポジトリで使う場合も、ユーザーの `config.toml` に `file:` の取得元を書いて各プロジェクトに取り込む（[spec.md 9.5](spec.md#95-スコープと取得元)）。

## 5. 人とエージェントの往復

```mermaid
flowchart LR
    H[人: VS Code で編集・保存] --> P[(diagram.drawio.png)]
    P --> X[エージェント: read]
    X --> E[編集用 XML<br/>登録画像は image=lib:…@指紋]
    E --> R[エージェント: write]
    R --> P
```

- 人が拡張のライブラリ欄から部品を置くと、`xml` のエントリーはそのセル（style の `image` を含む）が、`data` のエントリーは `convertDataUri` で変換した `image` の値を持つ画像セルが入る（`data` の変換は draw.io 26.0.2 の `app.min.js`、`xml` のセルの挿入は 31.4.6 の `EditorUi.addLibraryEntries` のソースで確認）。CLI の展開も同じ値を作るため、人が置いたアイコンもエージェントが置いたアイコンも、次の `read` で同じ `image=lib:` の参照に縮まる。
- 人が別の画像に差し替えた場合は、その画像の指紋で照合し直す。登録ライブラリにない画像は実データのまま残る。
- `write` の既定のレンダラー `vscode` は、拡張 1.9.0 が表示倍率 100% で保存した PNG に合わせる。フォント・OS の違いによる画素の差は残る（v0.1.0 の [描画互換性](rendering.md) と同じ）。
- 拡張で保存した PNG を `write` で作り直さなくても、`read` → 編集 → `write` の流れはそのまま使える。

## 6. draw.io の図形と拡張の「その他の図形」

CLI は draw.io のサイドバーの図形を同梱しない。エージェントに AWS・Azure・Kubernetes などの図形を使わせる場合は、利用者が使う図形を draw.io でライブラリにして `library add` で登録する（[usecase.md の I](usecase.md#drawio-の図形をライブラリにする)）。登録したライブラリは `customLibraries` に出るので、人も拡張のライブラリ欄から同じ図形を置ける。人は拡張の「+ その他の図形」で図形集を有効にして使ってもよいが、エージェントが参照できるのは登録した図形だけである。

AWS などの `mxgraph.*` の図形を CLI で描画するには、v0.1.0（`DIP_DRAWIO_WEB_PATH`）と同じく `D_PNG_DRAWIO_WEB_PATH` の資材が必要である。拡張は自身の資材で描画するため、人の側の準備は不要。

## 7. 注意と未検証の事項

- **ライブラリの更新の反映**: 拡張はライブラリファイルを図面を開くときに読み込む。`library add`・`library sync` の後は図面を開き直す（ファイルの変更を監視するかは未確認）。
- **拡張上でのライブラリの編集**: 設定から読み込んだライブラリを拡張上で編集・保存する流れは想定しない。部品の追加・変更は CLI（`library add --merge`）か draw.io Desktop で行い、Desktop で保存した場合は `library sync` を実行する。
- **リモート環境**: 拡張の `extensionKind` は `["ui", "workspace"]` で、リモート（devcontainer・WSL・SSH）では既定でローカル側で動く。拡張は `${workspaceFolder}` を `workspaceFolder.uri.fsPath` に置き換え、`Uri.file()` で読むため、ローカル側で動く場合はリモートのパスを読めない可能性がある。`"remote.extensionKind": {"hediet.vscode-drawio": ["workspace"]}` でリモート側で動かせば読める見込みだが、どちらも未検証である。
- **大きなライブラリ**: 拡張は設定したライブラリをすべて読み込んで webview に渡す。数 MB のライブラリを多数登録すると図面を開く時間が延びる。使うライブラリだけを登録する。
- **CI**: `library install --frozen` で取り込んだライブラリを復元し、`library sync --check` で、人が draw.io Desktop で保存して拡張で読めなくなったライブラリと、取り込みの漏れ・完全性の不一致を検出できる。
- **clone 直後**: 取り込んだライブラリ（`.d-png/libraries/@*/`）は Git に入っていないため、設定にあるファイルが存在せず、拡張はそのライブラリを読めない。`library install` を実行してから図面を開き直す。
