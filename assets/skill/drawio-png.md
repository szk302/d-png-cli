# drawio-png: drawio-png-cli で .drawio.png を扱う（vscode モード）

`.drawio.png` は、見た目の PNG 画像と、編集用の draw.io XML（PNG のテキストチャンク内）を 1 ファイルに持つ。
`drawio-png-cli` はこの XML を取り出し（extract）、検証し（validate）、編集後の XML から画像を描き直して書き戻す（embed）CLI。
PNG を Python などで直接いじると、圧縮形式の違いや画像との同期を取り違えやすい。必ず drawio-png-cli を経由する。

このスキルは、**VS Code の draw.io 拡張（1.9.0）で保存したのと同じ見た目**になる vscode モード
（Chrome / Chromium で描画）を使う。Desktop レンダラー、`raw`・`desktop` モード、フォント指定、
`--no-render` など、ここに無い機能が必要なときは `drawio-png-cli skill --full` を読む。
その場合も、ユーザーが別のレンダラー・モードを明示的に求めない限り、embed の
`--renderer chromium --chromium-mode vscode` は変えない。

## 準備

描画には Chrome（無ければ Chromium）が必要。見つからない場合は、導入または `DRAWIO_PNG_CHROME_PATH` の設定をユーザーに案内する。
この手順は `drawio-png-cli --version` の drawio-png-cli に対応している。

## 基本の流れ

一時ファイルはリポジトリの外（スクラッチ領域や `mktemp -d`）に置く。作業用 XML をリポジトリに残さない。

```sh
drawio-png-cli extract diagram.drawio.png -o "$tmp/diagram.xml"   # 全ページを非圧縮 XML で取得
# … XML を編集 …
drawio-png-cli validate "$tmp/diagram.xml"                         # 構造を検証
drawio-png-cli embed --renderer chromium --chromium-mode vscode \
  -i "$tmp/diagram.xml" -o diagram.drawio.png           # 先頭ページを描画し、全ページの XML を保存
```

- **embed には常に `--renderer chromium --chromium-mode vscode` を付ける。** 省略すると、draw.io Desktop が
  入っている環境では Desktop で描画され、VS Code 拡張と見た目が変わる。
- **読むだけなら extract だけでよい。** `-o` を省けば stdout に出る。図面の説明を求められたら、
  セルの `value`（ラベル）、`source`/`target`（接続）、`parent`（グループやコンテナ）、ページ名を読み取って答える。
- **大きな画像データを読み込まない。** アイコン入りの図面では、セルの `style` に `image=data:image/...` の
  Base64 が数千〜数万文字入ることがある。行単位の `grep` では同じ行の Base64 も出力されるので、
  必要な属性だけを取り出して読む: `grep -oE ' (id|value|label|source|target|parent|vertex|edge)="[^"]*"' "$tmp/diagram.xml"`
- **入力と同じパスへ embed してよい。** 一時ファイルに書いてから置き換えるので、失敗時は元のファイルが残る。
- **embed は保存前に自動で検証する。** 検証エラーなら何も書き換えないので、XML を直して再実行する。
- 終了コード: `0` 成功、`1` 処理・検証エラー、`2` 引数エラー。エラー内容は stderr に出る。

## XML を編集するときの注意

drawio-png-cli は図面の構造（`mxfile`/`mxGraphModel`、各ページの `root` 直下の `<mxCell id="0"/>` と
`<mxCell id="1" parent="0"/>`）を検証するが、見た目の崩れまでは検出しない。

- **必要な箇所だけ変える。** extract 結果はページ順・ページ名・未知の属性や要素を保持している。
  関係ないセル・ページ・属性を整形し直したり削ったりしない。
- **ID はページ内で一意にする。** 新しいセルには既存と重ならない ID を付ける。
  接続線（`edge="1"`）の `source`/`target` は既存セルの ID を正確に参照する。
- **既存図面のスタイルに合わせる。** 追加する図形は、近くの同種セルの `style`（色・フォント・角丸など）を真似ると図面になじむ。
- **新規作成**では次の最小構成から始める。

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

## アイコン・図形を入れる（ライブラリ）

ロゴやアイコン、AWS などの draw.io の図形は、ライブラリから探して入れる。style を推測で書かない。
ライブラリは 2 種類あり、どちらも同じコマンド（`drawio-png-cli library search` など）でまとめて扱う。

- **利用者のライブラリ**: draw.io のカスタムライブラリ（`<mxlibrary>` の XML。Simple Icons など）。
  環境変数 `DRAWIO_PNG_LIBRARY_PATH`（ファイルやディレクトリを `PATH` と同じ区切りで指定）で決まる。ユーザーがライブラリの
  ファイルを示した場合は、各コマンドに `--library-file <ファイル>` を付ける（このときは組み込みを使わない）。
- **組み込みライブラリ（`drawio/*`）**: VS Code 拡張 1.9.0 と同じ draw.io 26.0.2 のサイドバーの図形
  （AWS・Azure・Google Cloud（GCP2）・Kubernetes・UML など）。drawio-png-cli に同梱している。描画には後述の Web 資材が必要。

```sh
drawio-png-cli library search lambda                       # ライブラリ名・番号・タイトル・サイズ（style や画像データは出ない）
drawio-png-cli library style drawio/aws4-compute 16        # 単一セルの図形の style を 1 行で出力（XML を書くときに写す）
drawio-png-cli insert --library drawio/aws4-compute --index 16 -i "$tmp/diagram.xml" -o "$tmp/diagram.xml" \
  --id fn --x 40 --y 40                         # 挿入したセルの ID を出力
drawio-png-cli library preview lambda -o "$tmp/icons.png"  # 候補を画像で見比べる（任意）
```

- 検索語はタイトルの部分一致（大文字小文字は区別しない）。見つからなければ短い語や別名で探し直す。
- 検索結果の 1 列目（ライブラリ名）を `--library` に、2 列目（番号）を `--index` に写す。タイトルで選ぶなら
  `--name`（一意に決まる必要がある。複数のライブラリにあれば `--library` を付ける）。
- 図面を XML で丸ごと書くときは、組み込みの図形は `drawio-png-cli library style` の出力を `style="…"` にそのまま写す
  （XML の属性値としてエスケープ済みなので、`&quot;` などを戻さない）。
  `shape=` や `resIcon=` だけを抜き出さない。幅・高さは検索結果のサイズを使う（縦横比を保てば拡大・縮小してよい）。
- 画像のアイコン（利用者のライブラリの多く）や、カード・グループなど複数セルの図形は、style を写せない
  （`drawio-png-cli library style` はエラーになる）。`drawio-png-cli insert` で入れる。アイコンの `style` には大きな Base64 が入るので、自分で書き写さない。
- 組み込みには旧版の図形集（`drawio/aws3-*`・`drawio/aws3d`・`drawio/aws4b-*` など）も含まれる。ユーザーが求めない限り、
  AWS は `drawio/aws4-*`、Azure は `drawio/azure2-*`、Google Cloud は `drawio/gcp2-*`、Kubernetes は `drawio/kubernetes` を使う。
  同じものが利用者のライブラリにもあれば（最新の公式アイコンなど）、そちらを使ってよい。
- 見た目で選ぶ必要があるときだけ `drawio-png-cli library preview` を使う。各図形の上に「ライブラリ #番号」とタイトルが出る（図形自身のラベルは下に出る）。
  一度に 60 件まで。多すぎれば検索語か `--library` で絞る。組み込みの図形の描画には Web 資材が必要。
- 挿入後のラベル付けや接続線は、出力された ID（`--id` で指定可）を使って XML を編集する。
  `--label` でラベルも付けられるが、位置・書式はライブラリの `style` のまま（アイコンに重なることがある）。
- 組み込みの図形のうち、アイコンの画像データを含むもの（GCP2 の製品別アイコンなど）は収録していない。
  Google Cloud などのアイコンは利用者のライブラリから入れる。
- アイコン（ブランドロゴ）のライセンス・商標の条件は提供元に従う。

## 描画について

embed は**先頭ページだけ**を画像にする。2 ページ目以降を編集しても PNG の見た目は変わらない
（XML には全ページが保存される）。2 ページ目以降を変えたときは、そのことをユーザーに伝える。

描画に失敗したときは、勝手に描画を省略したり、レンダラーやモードを変えたりしない。原因をユーザーに伝える。

### AWS・Google Cloud（GCP2）などの図形

`shape=mxgraph.aws4.*`・`mxgraph.gcp2.*` などの draw.io 図形集は、描画用の資材を同梱していないので、
`Unsupported shape: ...` になる。VS Code 拡張 1.9.0 と同じ **draw.io 26.0.2**
（コミット `96a916a337d13fc8bf622c8a67d422bd284eabe5`）の Web 資材を `DRAWIO_PNG_DRAWIO_WEB_PATH` で指定する。
他のコミット（例: `f3abfe0f…` の 31.4.5）は使わない。VS Code 拡張と見た目が 1px ずれる。

```sh
git clone https://github.com/jgraph/drawio.git "$tmp/drawio"
git -C "$tmp/drawio" checkout 96a916a337d13fc8bf622c8a67d422bd284eabe5
DRAWIO_PNG_DRAWIO_WEB_PATH="$tmp/drawio/src/main/webapp" \
  drawio-png-cli embed --renderer chromium --chromium-mode vscode -i "$tmp/diagram.xml" -o diagram.drawio.png
```

- **`DRAWIO_PNG_DRAWIO_WEB_PATH` が既に設定されていれば、そのまま embed する。** 指定先のディレクトリは開かない
  （`ls`・`find`・`grep`、版や git コミットの確認、図形・アイコンの有無の確認をしない）。
  資材の確認は drawio-png-cli が行い、使えない資材ならエラーになる。図形の有無は、資材ではなく `drawio-png-cli library search` で調べる。
- 設定されていなければ、資材の場所をユーザーに尋ねるか、了承を得て上記のとおり clone する
  （clone は外部への通信）。ディスク上の draw.io 資材を探し回らない。
- 「Google Cloud Platform 2026」（`mxgraph.gcp3.*`）は 26.0.2 に無く、VS Code 拡張 1.9.0 でも表示されない。使わない。
- 画像を `data:` URL で埋め込んだ図形（Simple Icons、Google Cloud Icons など）は Web 資材なしで描ける。

図形の探し方・入れ方は、前述の「アイコン・図形を入れる（ライブラリ）」を参照。

### よくあるエラーと対応

| エラー | 対応 |
| --- | --- |
| `Chromium/Chrome not found` | Chrome / Chromium の導入、または `DRAWIO_PNG_CHROME_PATH` の設定をユーザーに案内する |
| `Unsupported shape: ...` | 図形集（AWS 等）が必要。上記の 26.0.2 の Web 資材を指定する。指定済みでも出る場合は、その図形・アイコン（`resIcon`・`grIcon` 等）が 26.0.2 に無い。資材を調べて確かめず、エラーの図形名をユーザーに伝える。図形を勝手に別の形へ置き換えない |
| `Math and automatic layout require ...` | 数式・自動レイアウトにも `DRAWIO_PNG_DRAWIO_WEB_PATH` が必要 |
| `external resource blocked` / 外部画像・フォントの取得エラー | 既定でネットワーク取得を禁止している。外部 URL への接続が問題ないかユーザーに確認してから `--allow-network` を付ける |
| `draw.io opened a ... dialog` | 指定した Web 資材がエラーを出している。資材の中身は調べず、`DRAWIO_PNG_DRAWIO_WEB_PATH` の指定先と版（26.0.2 か）の確認をユーザーに依頼する |
| `Chromium rendering timed out` | 描画が60秒以内に終わらない。資材や図面の大きさを確認する |
| `interrupted (signal N)` | 描画中に中断された。後片付け済みで出力は変わっていない |
| 検証エラー（`missing essential <mxCell id="0">` など） | XML を直す。`--no-validate` はデバッグ用なので使わない |

`--allow-network` は外部への通信を許可する。既定では Chrome 自身の Google への通信も止めているが、
`--allow-network` 時は一部残り、プロキシ環境ではプロキシ経由で外に出得る。必要なときだけ、ユーザーの了承を得て使う。

### VS Code 拡張との見た目の差

同じフォント環境なら、保存時の表示倍率が 100% の VS Code 拡張 1.9.0 と同じ見た目（寸法・配置・画素）になる。
VS Code（Windows など）と drawio-png-cli（Linux など）で OS やフォントが違うと、文字の幅が変わり、
画素だけでなく画像の寸法や配置も変わり得る。完全な一致は保証しない。

## 仕上げ

- 変更後は `drawio-png-cli validate diagram.drawio.png` で書き戻した結果を確認できる。
- 報告では、何を変えたか（追加・変更したセル、挿入したアイコン）と、描画の有無
  （先頭ページのみ描画、`DRAWIO_PNG_DRAWIO_WEB_PATH` を使った場合はその版）を伝える。
