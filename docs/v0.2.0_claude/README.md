# drawio-png-cli v0.2.0 仕様案（Claude）

v0.2.0 の `drawio-png-cli` を、**エージェントが draw.io の XML を書いて標準入力で渡すと `.drawio.png` ができる**道具として作り直す案。v0.1.0 との互換性は考えない。

| 文書 | 内容 |
| --- | --- |
| [spec.md](spec.md) | CLI の仕様（共通の規約、コマンド、XML の参照、ライブラリの登録） |
| [vscode.md](vscode.md) | VS Code 拡張（`hediet.vscode-drawio` 1.9.0）との連携と、ライブラリ書式の制約 |
| [usecase.md](usecase.md) | ユースケース A〜I。各ケースで「CLI に渡す XML」と「CLI がない場合の XML」を並べて比較する |
| [examples/](examples/) | 各ケースの `cli.xml`・`no-cli.xml`・描画結果の `expected.drawio.png`、比較用ライブラリ、VS Code の設定 |

## 最小の使い方

```sh
drawio-png-cli init                                   # .drawio-png/ を作る
drawio-png-cli library add ./simple-icons.xml         # プロジェクト専用のライブラリを登録する
drawio-png-cli library add ./shapes.xml               # draw.io の図形も、ライブラリにして登録する
drawio-png-cli library install @acme/brand            # 社内の共有ライブラリを取り込む（取得元は config.toml の [scopes]）
drawio-png-cli library vscode --write                 # VS Code 拡張でも同じライブラリを使う
drawio-png-cli library search postgres                # simple-icons/postgresql  image  48x48  PostgreSQL

drawio-png-cli write -o arch.drawio.png <<'EOF'
<mxfile>
  <diagram id="page-1" name="構成図">
    <mxGraphModel>
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <mxCell id="orders" value="注文 DB" parent="1" libref="shapes/cylinder">
          <mxGeometry x="40" y="40" as="geometry"/>
        </mxCell>
        <mxCell id="db" value="DB" parent="1" libref="simple-icons/postgresql">
          <mxGeometry x="200" y="56" width="48" as="geometry"/>
        </mxCell>
        <mxCell id="e" style="endArrow=block;html=1;" edge="1" parent="1" source="orders" target="db">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
EOF
```

## 主な設計判断

1. **`insert` を廃止し、XML に参照を書く。** エージェントは図面全体を XML で書くため、1 図形ずつコマンドで挿入して ID を受け取る流れは合わない。部品は `libref="<ライブラリ>/<ID>"` のプレースホルダーとして書き、`write` が展開する。1 回の呼び出しで図面ができ、中間ファイルも要らない。
2. **参照は 1 つの文字列にする。** 登録時にエントリーへ安定した ID を付けるため、タイトルの重複・空・`/` を含むタイトル・番号のずれを考えずに済む。PRD 4.2 で計画していた「ライブラリ名・タイトル・番号」と「画像ハッシュ」の 2 系統の指定は、同じ参照文字列の 2 つの書き方（セル群の `libref` と画像だけの `image=lib:`）にまとめた。
3. **ライブラリの登録を必須にし、ライブラリを同梱しない。** 参照はプロジェクトに登録・取り込みしたライブラリだけで解決する。draw.io のサイドバーの図形（AWS・Azure など）も同梱せず、使う図形は利用者が draw.io でライブラリにして登録する。図形集・アイコンにはそれぞれのライセンス・商標の条件があり、何を使うかは利用者が決めるためである。環境変数で任意のパスを指すのをやめ、どこで実行しても（CI・他の開発者の環境でも）同じ部品になるようにした。登録したファイルはそのまま VS Code 拡張の設定から読ませる。
4. **ライブラリ名に npm と同じ名前空間を入れる。** スコープのない名前（`components`）はそのプロジェクト専用、スコープ付きの名前（`@acme/brand`）は取得元から取り込む共有ライブラリとする。スコープごとに取得元（ディレクトリ・Git・HTTPS）を設定し、`library install` で取り込んで版を `lock.json` に固定する。同じ名前の部品集が社内・個人にあっても参照が衝突せず、版を上げるまで部品が変わらない。公開・認証を扱うレジストリーサーバーは作らず、Git などの既存の手段で配布する（[spec.md 9.5](spec.md#95-スコープと取得元)）。
5. **登録ファイルは VS Code 拡張が読める書式で書く。** 拡張 1.9.0 は `<mxlibrary>` の中身を実体参照を解かずに `JSON.parse` するため、CDATA や `&lt;` を使ったライブラリは draw.io で読めても拡張では読めない。CLI は XML の特殊文字を JSON 側のエスケープ（`\u003c` など）で書き、draw.io と拡張の両方で同じ内容になるようにする（[vscode.md](vscode.md#3-ライブラリファイルの書式の制約)）。
6. **保存する XML には参照を残さない。** PNG の中は普通の draw.io XML なので、VS Code・draw.io で開くのに CLI もライブラリも要らない。
7. **`read` と `expand` を逆向きの対にする。** `read` は登録済みの画像を指紋付きの `image=lib:` に縮めてエージェントのコンテキストを節約し、`expand` は参照を展開した単体の XML を出す。v0.1.0 の `--keep-images` のようなオプションは要らない。
8. **既定のレンダラーを VS Code 互換にする。** `--renderer` と `--chromium-mode` を 1 つにまとめ、既定を `vscode` にした。draw.io Desktop が入っているかどうかで結果が変わらない。
9. **PNG を読む・書くコマンドを `read`・`write` と呼ぶ。** ツール名と合わせて「drawio-png を読む・書く」と読め、標準出力・標準入力との向きも名前どおりになる。描画しない `write --metadata-only` でも名前と矛盾しない。`write` が描画を伴うことはヘルプと手順書で明記する（[spec.md 3 章](spec.md#3-コマンド一覧)）。
10. **入出力・出力形式・エラーを全コマンドで揃える。** 入力は位置引数か標準入力、出力は `-o`、機械向けは `--json`、エラーはコード付きで全件をまとめて報告する（[spec.md 2 章](spec.md#2-共通の規約)）。

## 確認したこと・していないこと

- VS Code 拡張 1.9.0（タグ `v1.9.0`）のソースで、`customLibraries` の読み込み方法と `extensionKind` を確認した。拡張が使う `xml-parser-xo` 3.1.0 と同じ手順で、例のライブラリと実物の Simple Icons for draw.io 16.35.0 を読み、CDATA 形式が失敗し、CLI の書式が成功することを確かめた。実体参照を使った書式は、パーサーのソースから読めないと判断した（実行はしていない）。
- draw.io 26.0.2 の `app.min.js` と 31.4.6 のソースで、`data` エントリーの画像の変換（`convertDataUri`）とライブラリの保存処理を確認した。
- B〜E・G の「CLI がない場合の XML」は、0.1.0 系の CLI で `validate` を通し、Chrome の vscode モード（draw.io 26.0.2 の Web 資材）で描画した（`expected.drawio.png`）。利用者ライブラリの部品は 0.1.0 系の `insert` でも展開し、ID・位置・サイズ・画像の値が一致することを確かめた。A・F・H は v0.2.0 の CLI の `write`（vscode、同梱資材のみ）で描画した。全ケースの展開結果は `tests/acceptance.rs` で検査している。
- 名前空間の例（[examples/acme-drawio-libraries/](examples/acme-drawio-libraries/)・[lock.json](examples/project/.drawio-png/lock.json)）の完全性の値は、例のファイルのバイト列から計算した。取得元からの取り込み・`library pack` は実装しておらず、npm との対応は仕様上の対応である。
- v0.2.0 は実装していない。CLI の出力例（検索結果・`--json`）は仕様から作ったもので、実行結果ではない。VS Code 上での操作、リモート環境（devcontainer・WSL）での拡張の動作は確認していない。
