# d-png: d-png-cli で .drawio.png を作る・直す

`.drawio.png` は、見た目の PNG 画像と、編集用の draw.io XML（PNG のテキストチャンク内）を 1 ファイルに持つ。
`d-png-cli` は、**draw.io の XML を標準入力で受け取って `.drawio.png` を書く**（write）CLI。
既存の `.drawio.png` からは編集用の XML を取り出せる（read）。
PNG を Python などで直接いじると、圧縮形式の違いや画像との同期を取り違えやすい。必ず d-png-cli を経由する。

描画は既定で **VS Code の draw.io 拡張（1.9.0）で保存したのと同じ見た目**になる（`vscode` レンダラー。Chrome / Chromium を使う）。
ユーザーが求めない限り `--renderer` は変えない。

## 準備

描画には Chrome（無ければ Chromium）が必要。見つからない場合は、導入または `D_PNG_CHROME_PATH` の設定をユーザーに案内する。
この手順は `d-png-cli --version` の d-png-cli に対応している。

## 基本の流れ

```sh
# 新しい図面: XML を書いて標準入力で渡す
d-png-cli write -o diagram.drawio.png <<'EOF'
<mxfile>…</mxfile>
EOF

# 既存の図面: 読む → 編集 → 書く（一時ファイルはリポジトリの外に置く）
d-png-cli read diagram.drawio.png -o "$tmp/diagram.xml"
# … XML を編集 …
d-png-cli write "$tmp/diagram.xml" -o diagram.drawio.png
```

- **write は保存前に参照の展開と検証を行う。** 問題があれば何も書き換えず、すべての問題をコード付きで報告する。
  直してから再実行する。入力と同じパスへ書いてよい（一時ファイルから置き換える）。
- **確認だけなら validate。** `d-png-cli validate diagram.xml` は write の描画前までを行い、何も書かない。
- **read は登録済みのアイコン画像を `image=lib:<参照>@<指紋>` に縮める。** Base64 を読まずに編集でき、write で元の画像に戻る。
  その値は書き換えない。ライブラリにない画像はそのまま残るので、行単位の `grep` で Base64 を読み込まないよう、
  必要な属性だけを取り出す: `grep -oE ' (id|value|label|source|target|parent|vertex|edge|dpng-ref)="[^"]*"' "$tmp/diagram.xml"`
- **画像を含む単体の XML が必要なら expand。** `d-png-cli expand diagram.drawio.png` は参照を展開した、draw.io でそのまま開ける XML を出す。
- 終了コード: `0` 成功、`1` 入力・参照・描画・保存のエラー、`2` 引数のエラー。`--json` を付けると結果を JSON で受け取れる。

## XML を書くときの注意

- 各ページの `root` 直下に `<mxCell id="0"/>` と `<mxCell id="1" parent="0"/>` が必要。
- **ID はページ内で一意にする。** `parent`・`source`・`target` は同じページにあるセルの ID を正確に参照する。
- **既存の図面は必要な箇所だけ変える。** read の結果はページ順・名前・未知の属性や要素を保持している。
- **既存図面のスタイルに合わせる。** 追加する図形は、近くの同種セルの `style` を真似ると図面になじむ。

```xml
<mxfile>
  <diagram name="Page-1" id="page-1">
    <mxGraphModel>
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <mxCell id="a" value="開始" style="rounded=1;whiteSpace=wrap;html=1;" vertex="1" parent="1">
          <mxGeometry x="40" y="40" width="120" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="b" value="終了" style="rounded=1;whiteSpace=wrap;html=1;" vertex="1" parent="1">
          <mxGeometry x="240" y="40" width="120" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="e1" style="edgeStyle=orthogonalEdgeStyle;html=1;" edge="1" parent="1" source="a" target="b">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
```

## アイコン・図形はライブラリの参照で書く

draw.io の図形（AWS など）、ロゴやアイコンは、style や Base64 を推測で書かず、プロジェクトに登録されたライブラリの**参照文字列**で書く。
CLI はライブラリを同梱していない。必要な図形が見つからなければ、ユーザーに登録（`library add`）を依頼する。

```sh
d-png-cli library search cylinder             # 参照・種類・サイズ・タイトル。先頭の列が参照文字列
#   shapes/cylinder	shape	60x80	Cylinder
d-png-cli library show shapes/cylinder        # style、展開後のセルと ID、配置例
d-png-cli library preview cylinder -o "$tmp/candidates.png"   # 候補を並べた画像（目で選ぶとき）
```

参照文字列は `<ライブラリ>/<エントリー>`（プロジェクトのライブラリ）か `@<スコープ>/<ライブラリ>/<エントリー>`
（取り込んだ共有ライブラリ）。検索結果からそのまま写し、推測で作ったりスコープを補ったりしない。

```xml
<!-- エントリーのセルごと置く。位置・ラベル・style（キー単位で上書き）・サイズを指定できる -->
<mxCell id="orders" value="注文 DB" parent="1" dpng-ref="shapes/cylinder">
  <mxGeometry x="40" y="40" as="geometry"/>
</mxCell>
<!-- 画像だけを使う。style とサイズは自分で書く -->
<mxCell id="db" value="DB" style="shape=image;image=lib:simple-icons/postgresql;aspect=fixed;" vertex="1" parent="1">
  <mxGeometry x="200" y="40" width="48" height="48" as="geometry"/>
</mxCell>
```

- **種類で書けるものが決まる**（`library search` の 2 列目）。`image`・`shape` は幅・高さを指定できる（片方なら縦横比を保つ）。
  `edge` は `source`・`target` を書く。`group`・`set` は位置だけ。迷ったら `vertex`・`edge` 属性は書かない。
- **複数セルのエントリー**は、最上位のセルがプレースホルダーの ID になり、子は `<ID>-1`、`<ID>-2`… になる。
  最上位が複数（`set`）なら全セルが `<ID>-1`… になり、`<ID>` 自体は残らない。中のセルへ接続するときは
  `library show <参照>` の `cells` で ID を確かめる。
- 保存される XML には参照は残らず、普通の draw.io XML になる。
- AWS などの `mxgraph.*` の図形の描画には、draw.io 26.0.2 の Web 資材（`D_PNG_DRAWIO_WEB_PATH`）が必要。
  `ASSET_MISSING` になったらユーザーに用意を依頼する。
- 必要な部品がライブラリにない場合は、ユーザーにライブラリの登録を依頼する（ライブラリのファイルや画像を渡されたら `d-png-cli library add <ファイル>` で登録できる）。勝手に別の部品で代用しない。

## エラーへの対処

| コード | 行うこと |
| --- | --- |
| `REF_NOT_FOUND` | `candidates` を確認し、`library search` で正しい参照文字列を探す |
| `REF_KIND` | `image=lib:` を画像以外に使っていないか、`vertex`・`edge` の指定が種類と合っているか確かめる |
| `REF_OVERRIDE` | `library show` の種類を見て、許されない幅・高さ・ラベル・style の指定を外す |
| `REF_ID_CONFLICT` | プレースホルダーの ID を変える。`set` の ID を参照していれば `<ID>-n` に直す |
| `ID_DANGLING`・`ID_DUPLICATE` | `parent`・`source`・`target` の参照先と ID の重複を直す（全件まとめて報告される） |
| `IMAGE_CHANGED` | read の後でライブラリの画像が変わった。元の画像が必要ならユーザーに相談する |
| `LIBRARY_NOT_INSTALLED`・`LIBRARY_INTEGRITY` | 名前空間付きのライブラリが取り込まれていない・変わっている（clone 直後に多い）。`d-png-cli library install` を実行する |
| `RENDERER_NOT_FOUND`・`ASSET_MISSING` | Chrome や描画資材の用意をユーザーに依頼する。別の部品に置き換えない |

## 完了の確認

write が成功したら、**PNG を画像として開き**、図形・アイコンの種類、欠け・変形、ラベルの重なり、線の接続と矢印を確認する。
2 ページ目以降を変えた場合は `--page N` で別のファイルに描いて確認する。
ユーザーには、出力ファイル、変更した内容、見た目の確認結果を伝える。
