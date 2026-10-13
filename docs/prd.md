# プロダクト要件定義書 (PRD): `drawio-png-cli`

## 1. 概要 (Overview)

`drawio-png-cli` は、VS Code の draw.io 拡張機能などで扱われる `.drawio.png` 形式のファイルを、AI エージェントと人間がシームレスに相互編集できるようにするための軽量かつ高速な CLI ツールである。

PNG 画像のメタデータ（`zTXt` / `tEXt` チャンク）内に埋め込まれた draw.io の XML データを抽出し、AI による編集後の XML を安全に再埋め込み・画像化するインターフェースを提供する。

---

## 2. 目的・背景 (Background & Goals)

* **課題**: `.drawio.png` 形式はバイナリ（PNG画像）とテキスト（XMLメタデータ）が混在しているため、プログラムや LLM / AI エージェントが直接読み書きを行うことが困難であった。
* **目的**:
1. AI エージェントが安全・確実に draw.io の構造データ（XML）を取り出し、修正・更新できるようにする。
2. 人間が VS Code の GUI で編集した `.drawio.png` と、AI がプログラム経由で作成・更新した構造データを双方向に同期・補完できるようにする。
3. CLI 実行時に XML の構文・基本構造を検証し、不正な入力による上書きを防ぐ。すべての描画・レイアウト不具合の防止は保証しない。



---

## 3. ユースケース (Use Cases)

```
[人間 (VS Code)] ──(GUIで作成)──> filename.drawio.png
                                         │
                                   [drawio-png-cli extract]
                                         │
                                         ▼
[AI エージェント] <──(XML解析・修正)─── diagram.xml
       │
 [drawio-png-cli embed] ──(自動Validation)──> filename.drawio.png
                                         │
[人間 (VS Code)] <──(GUIで微調整)────────┘

```

1. **AI による既存図面の自動アップデート**: 人間が作成した `.drawio.png` から XML を抽出し、AI が図面構造を追加・修正した上で `.drawio.png` へ書き戻す。
2. **AI による新規図面の生成**: AI が一から生成した XML データを `drawio-png-cli embed` に引き渡し、VS Code で即座に開いて人間が微調整できる `.drawio.png` ファイルを出力する。
3. **CI/CD パイプラインでの図面検証**: PR 提出時などに `drawio-png-cli validate` を通すことで、破壊された draw.io ファイルの混入を防ぐ。

---

## 4. CLI インターフェース仕様 (CLI Specification)

* **コマンド名**: `drawio-png-cli`
* **リポジトリ名**: `drawio-png-cli`

### 4.1. サブコマンド一覧

#### `drawio-png-cli extract`

`.drawio.png` から XML メタデータを抽出し、全ページを非圧縮 XML に展開して出力する。ページ順・名前・属性・未知要素を保持するが、元の文字列表現との完全一致は保証しない。（計画）ライブラリ参照の画像は既定で印に置き換える（[4.2](#42-ライブラリ参照属性計画未実装)）。

| オプション | 型 | 説明 |
| --- | --- | --- |
| `input` (位置引数) | Path | 対象の `.drawio.png` ファイルパス |
| `-o`, `--output` | Path | 出力先 XML ファイルパス（省略時は標準出力 `stdout`） |
| `--keep-images` | Flag | （計画）ライブラリ参照の画像を印に置き換えない。[4.2](#42-ライブラリ参照属性計画未実装) を参照 |
| `--raw` | Flag | （計画）PNG に保存された XML を、ページの展開・検証・参照の処理をせずに出力する。[4.2](#42-ライブラリ参照属性計画未実装) を参照 |

* **実行例**:
```bash
# ファイルに出力
drawio-png-cli extract input.drawio.png -o diagram.xml

# パイプラインで標準出力受け取り
drawio-png-cli extract input.drawio.png > diagram.xml

```



---

#### `drawio-png-cli embed`

XML データを `.drawio.png` に挿入・更新する。**実行時に自動でバリデーションを呼び出し、パスした場合のみ書き込む。**

| オプション | 型 | 説明 |
| --- | --- | --- |
| `-i`, `--input` | Path | 入力 XML ファイルパス（省略時は標準入力 `stdin`） |
| `-o`, `--output` | Path | (必須) 出力先 `.drawio.png` ファイルパス |
| `-b`, `--base-image` | Path | ベースとなる既存 PNG ファイルパス。`--no-render` と併用する |
| `--no-render` | Flag | 画像の再描画を行わず、メタデータ（XML）のみを更新する(このオプションを未指定かつ外部レンダラーが存在しない場合、警告終了ではなくエラー終了させる) |
| `--no-validate` | Flag | （デバッグ用）自動バリデーションをスキップして強制書き込みする |

`--no-render` でベース画像を省略した場合は透明な 1×1 PNG を生成する。既存の出力先はベースとして再利用しない。画像と XML が同期されない旨を stderr に警告する。複数ページはすべて保持し、通常の描画では先頭ページを画像化する。

`--no-validate` は XML 検証と正規化を省略するが、UTF-8・サイズ制限・PNG 検査・アトミック保存は省略しない。

* **実行例**:
```bash
# XMLファイルから埋め込み
drawio-png-cli embed -i modified.xml -o output.drawio.png

# stdin 経由でパイプライン結合
cat modified.xml | drawio-png-cli embed -o output.drawio.png

```



---

#### `drawio-png-cli validate`

指定した XML または `.drawio.png` ファイルが正当な draw.io 構造を持っているか事前検証する。

* **実行例**:
```bash
drawio-png-cli validate diagram.xml
drawio-png-cli validate output.drawio.png

```

---

### 4.2. ライブラリ参照属性（計画・未実装）

#### 背景

AI エージェントは図面を XML で丸ごと書くため、1 図形ずつ座標を指定して挿入する `drawio-png-cli insert` は作業の流れに合わない。一方、画像を埋め込んだエントリー（Simple Icons 等）は style に大きな Base64 を含み、エージェントが書き写せない。また一度挿入すると、`extract` のたびに Base64 が XML に現れ、エージェントのコンテキストを圧迫する。

そこで、エージェントは XML の中に「どのライブラリのどのエントリーか」を示す参照を書き、`embed` がそれを展開する。画像を埋め込んだセルだけは参照を残し、`extract` で画像を短い印に置き換えて、`embed` で戻す（往復する）。それ以外のエントリーは、`insert` と同じく一度だけ展開する。

#### エントリーの分類

エントリーを単一セルの XML にしたもの（`data` のエントリーは `insert` と同じく `shape=image;…;image=…` のセルにする）で分類する。

| 分類 | 条件 | 扱い |
| --- | --- | --- |
| 画像のエントリー | 単一のセルで、style の `image` キー（`=` のあるもの。名前付きスタイルの `image` ではない）の値が `data:` で始まる | 往復する |
| その他のエントリー | 上記以外（図形の単一セル、`image=img/lib/…` のようなパスの画像、`<object>` で包まれたセル、接続線、複数セル） | 一度だけ展開する |

Simple Icons（3,464 エントリー）は、すべて `xml` の単一セルで `image=data:image/svg+xml,…` を持つため、画像のエントリーになる。組み込みライブラリに `data:` を含むエントリーはないため、すべてその他のエントリーになる。

#### 属性

| 属性 | 内容 | エージェントが書くか |
| --- | --- | --- |
| `drawioPngLibrary` | ライブラリ名 | 書く（必須） |
| `drawioPngEntry` | エントリーのタイトル | 書く（必須） |
| `drawioPngIndex` | ライブラリ内の番号（1 から）。タイトルの重複・空のときに指定する | 必要なときだけ書く |
| `drawioPngHash` | 画像のハッシュ（`sha256:<小文字の16進数>`）。画像のエントリーにだけ `embed` が書く | 書かない |

* `drawioPng` で始まる属性を 1 つでも持つセルを参照とみなす。`drawioPngLibrary`・`drawioPngEntry` のどちらかが欠けていればエラーにする（属性の一部だけが残ったセルで、画像が黙って失われるのを防ぐため）。
* 参照の属性は 1 つの要素にまとめて書く。`<mxCell>` か、セルを包む `<object>`・`<UserObject>` のどちらでもよい。複数の要素に分かれている場合はエラーにする。
* 往復する参照（画像のエントリー）は、`embed` が `<object>` に書いて保存する。セルが包まれていなければ `<object>` で包む（`id` と `value` は `<object>` の `id`・`label` に移す）。理由:
  * draw.io は `<mxCell>` の未知の属性のうち、数値に見える値（`007`・`1.50`）を数値として読み込み、正規の表記（`7`・`1.5`）で書き戻す。`<object>` の属性は変換しない（[保持の確認](#drawio-での保持の確認)）。
  * `<object>` の属性は draw.io の「データを編集」に表示されるため、人がセルが参照であることに気付き、属性を消して参照を外せる。
  * draw.io が公式に用意した独自データの置き場所である。
* 参照を style のキーにしないのは、draw.io の「スタイルを編集」に表示され、人が style を書き換えたときに消えやすいため。
* draw.io の操作でセルが複製されると属性も複製されるため、属性にはセル固有の情報（ID 等）を入れない。

#### 参照の解決

1 つの文字列にまとめないのは、ライブラリ名が `/` を含み（`drawio/aws4-compute`）、タイトルも `/`・`#` を含む・空・重複することがあるため。組み込みライブラリ（12,671 エントリー）では、同じライブラリ内のタイトルの重複が 700 組（1,806 エントリー、145 ライブラリ）、空のタイトルが 71、`/` か `#` を含むタイトルが 152 あり、1,421 のタイトルが複数のライブラリにある（`Lambda` は `drawio/aws3-compute`・`drawio/aws3d`・`drawio/aws4-compute`）。

* `drawioPngLibrary` は `--library` と同じ名前の規則で解決する。省略は許さない（利用者のライブラリを先に探す解決順により、`DRAWIO_PNG_LIBRARY_PATH` の設定で別のエントリーになるのを避けるため）。
* `drawioPngEntry` は `--name` と同じく、完全一致、なければ大文字小文字を区別しない一意の一致で選ぶ。`<mxCell>` に書かれた値が draw.io で数値に変換された場合に備え、エントリーのタイトルを同じ規則（`parseFloat` してから文字列にする）で変換した値との一致も認める。
* 空のタイトルは `drawioPngEntry=""` と書き、`drawioPngIndex` を必須とする。
* `drawioPngIndex` を指定した場合は、その番号のエントリーを選び、そのタイトルが `drawioPngEntry` と上記の規則で一致することを確かめる（ライブラリの更新で番号がずれたことを検出するため）。番号だけでの指定は許さない。
* 往復する参照は、解決した結果で `drawioPngLibrary` を正式な名前（`aws4-compute` → `drawio/aws4-compute`）に、`drawioPngEntry` を正確なタイトルに書き換えて保存する。
* 次の場合は参照を解決できないものとする。ライブラリ・エントリーが見つからない。タイトルが一意に決まらず `drawioPngIndex` がない（候補の番号を示す）。番号が範囲外、または番号のエントリーのタイトルが一致しない。`drawioPngIndex` が正の整数でない。
* ライブラリは、XML に参照があるときだけ、参照されたものだけを読む。利用者のライブラリ名は `/` を含まないため、`drawio/` で始まる名前は `DRAWIO_PNG_LIBRARY_PATH` を読まずに組み込みライブラリから解決する。参照を含まない図面の `embed` の速度と、関係のない `DRAWIO_PNG_LIBRARY_PATH` の不備による失敗を避けるため。

#### 一度だけの展開（その他のエントリー、および画像のエントリーの初回）

エージェントは 1 つのセル（プレースホルダー）を書く。`embed` は、プレースホルダーをエントリーのセル群に置き換える。処理は `insert` と同じで、エントリーの要素・属性（style・ラベル・`<object>` のデータ・接続線のジオメトリ）をそのまま写す。

```xml
<!-- エージェントが書く XML -->
<mxCell id="fn" value="Handler" vertex="1" parent="1" drawioPngLibrary="drawio/aws4-compute" drawioPngEntry="Lambda">
  <mxGeometry x="40" y="40" as="geometry" />
</mxCell>
```

* **位置**: セル群の左上をプレースホルダーの `x`・`y`（省略時は 0）に合わせ、`parent` はプレースホルダーのものを引き継ぐ。接続線のエントリーは、`insert` と同じく点を移動する。
* **ID**: 最上位のセルが 1 つなら、そのセルにプレースホルダーの ID を付ける。他のセルは `<ID>-<n>` とし、`<n>` はエントリー内の文書順に 1 から振る。最上位のセルが複数のエントリーでは、すべてのセルを `<ID>-<n>` とし、プレースホルダーの ID を `source`・`target`・`parent` に使うセルがページにあればエラーにする。ページ内の既存 ID と重なればエラーにする。番号の振り方が決まっているため、エージェントは `embed` の前に子のセルやポートへの接続線を書ける。
* **接続**: エントリー内の接続先・親子関係は振り直した ID に対応付ける。図面の接続線がプレースホルダーの ID を指していれば、最上位のセルにつながる。
* **上書き**（`insert` の `--width`・`--height`・`--label` と同じ制約）:
  * `width`・`height`: 単一の図形のセルに限る。片方だけなら縦横比を維持する。
  * `value`（包まれていれば `label`）: 最上位のセルが 1 つのエントリーに限る。省略時はエントリーのラベルを使う。
  * `style`: 最上位のセルが 1 つのエントリーに限り、キー単位で最上位のセルの style に重ねる（同じキーはプレースホルダーの値を優先する）。名前付きスタイル（`=` のないトークン）を書いた場合はエラーにする。
  * 制約に反する指定はエラーにする。
* **参照の属性**: その他のエントリーでは、展開したセルに残さない。以後は普通のセルで、ライブラリの更新は反映されず、`embed` でライブラリを読むこともない（draw.io での人の変更を補完し直しで戻さないため）。画像のエントリーでは、次の往復の規則に従って残す。
* 画像を含む複数セルのエントリー（利用者のライブラリにあり得る）は、その他のエントリーとして一度だけ展開されるため、展開後の `extract` で Base64 が出力される。

#### 画像の往復（画像のエントリー）

* **ハッシュ**: XML の属性のエスケープを解いた style の `image` キーの値（`data` のエントリーでは `;base64,` を `,` にした後の文字列）を UTF-8 にしたものの SHA-256。
* **`extract`**: 参照の属性（`drawioPngLibrary`・`drawioPngEntry`・`drawioPngHash`）がそろい、`image` のハッシュが `drawioPngHash` と一致するセルは、`image` の値を印 `drawio-png:<drawioPngHash の値>`（例: `image=drawio-png:sha256:…`）に置き換える。キーを消さずに印にするのは、人が draw.io で画像を消したセルと区別するため。それ以外のセルは変更しない（扱いは `embed` で決める）。置き換えたセルがあれば、stderr に件数と `--keep-images` の案内を 1 行出す。
* **`embed`**: セルの状態に応じて次のとおり扱う。判定を `embed` に集めることで、`extract` のどのオプションで取り出した XML でも同じ結果になる。

| セルの状態 | ライブラリで解決できる | 解決できない |
| --- | --- | --- |
| `drawioPngHash` なし（エージェントが新しく書いた参照） | 一度だけの展開と同じくエントリーを写し、`image` と `drawioPngHash` を設定する | エラー |
| `image` が印で、`drawioPngHash` と一致（`extract` で置き換えた） | `image` を最新のエントリーの値にし、`drawioPngHash` を書き直す | エラー（`--keep-images` で取り出し直すよう案内する） |
| `image` が `data:` で、ハッシュが `drawioPngHash` と一致（`--keep-images`・`--raw` で取り出した） | 同上 | `image` を保持し、警告する |
| `image` のハッシュが `drawioPngHash` と一致しない（人が画像を差し替えた） | `image` を保持し、参照の属性を取り除いて警告する | 同左 |
| `image` キーがない（人が画像を消した） | 参照の属性を取り除いて警告する | 同左 |
| 印が `drawioPngHash` と一致しない、または参照でないセルに印がある | エラー | エラー |

* 往復するのは `image` の値だけで、style の他のキー・ラベル・ジオメトリは初回の展開の後は人やエージェントのものとして扱い、補完し直さない。
* ライブラリの更新でエントリーの画像が変わった場合（`drawioPngHash` と最新の画像のハッシュが異なる場合）は、新しい画像に置き換え、stderr に警告する。ライブラリの更新に追従する方針とし、固定はしない。

#### 既存のコマンド・オプションとの関係

* **`embed`**: 参照の展開は、`--no-render` を含め、検証の前に行う。展開できない場合は既存出力を変更せずエラー終了する。`--default-font` は展開の後に適用する（エントリーの style にある `fontFamily` は、既存の明示指定として保持される）。
* **`embed --no-validate`**: 参照を処理しない（XML の解析を省くデバッグ用のため）。プレースホルダーや印はそのまま保存される。
* **`validate`**: ライブラリを読まずにできる参照の検査を行う。`drawioPngLibrary`・`drawioPngEntry` の欠落、正の整数でない `drawioPngIndex`、複数の要素に分かれた属性、`drawioPngHash` と一致しない印・参照でないセルの印をエラーにする。参照が解決できるか（ライブラリの内容）は検査しない。
* **`extract`**: 既定の出力では画像を印に置き換える。単体で draw.io で開ける XML が必要な場合は `--keep-images` を使う。

#### `extract` のオプション

| オプション | ページ | 画像（`image=data:…`） | 用途 |
| --- | --- | --- | --- |
| なし | 非圧縮に展開する | ハッシュが一致すれば印に置き換える | エージェントの通常の編集 |
| `--keep-images` | 非圧縮に展開する | 残す（参照の属性も変更しない。従来の `extract` と同じ出力） | ライブラリがない環境での編集・保存、単体で開ける XML |
| `--raw` | 保存されたまま | 残す | 診断、PNG の内容の確認 |

* `--raw` は、PNG のチャンクから図面のメタデータを復号（`zTXt` の展開、旧 raw DEFLATE の展開、URL エンコードの解除）した XML をそのまま出力する。ページの展開・XML の検証・参照の処理は行わない（壊れた図面の診断にも使えるようにするため）。チャンクの検査と、競合するメタデータの拒否は従来どおり行う。drawio-png-cli の `embed` はページを展開して保存するため、圧縮されたページが出力されるのは draw.io で保存した場合である。
* `--raw` と `--keep-images` の併用は引数エラー（終了コード `2`）にする。
* `embed` は圧縮ページを展開してから参照を処理するため、どのオプションで取り出した XML も `embed` に渡せる。

#### `insert` の扱い

参照で `insert` の機能（画像・図形・複数セルのエントリーの挿入、ID の振り直し、エントリーの要素の写し、サイズ・ラベルの指定）を代替する。代替の仕方が異なる点:

* `insert` は新しい ID を出力する。参照では、ID の振り方（上記）が決まっているため、`embed` の前に分かる。
* `insert --library-file` に相当するものはない。そのファイルを `DRAWIO_PNG_LIBRARY_PATH` に加える。`--no-builtin` は、`drawioPngLibrary` でライブラリを必ず指定するため不要。
* `insert` は XML だけを書くが、`embed` は PNG を書く。描画が不要なら `embed --no-render` を使う。

時期: 参照を入れる版で `insert` の実行時に非推奨の警告を出し、その次のマイナー版で削除する。現在の `Cargo.toml` は `0.1.0` で、次の版には互換性のないコマンド名の変更（`dip` → `drawio-png-cli`）が既に入っているため、参照を入れる版を 0.2.0 とし、削除は 0.3.0 とする。削除にあわせて、`insert` を案内している次の箇所を変更する: `library style` のエラーメッセージ（`src/main.rs`）、README、スキル（`assets/skill/drawio-png.md`・`drawio-png-full.md`）。

#### `library style` とスキルの案内

* `library style` は残す。参照の属性を使わず draw.io の通常の XML だけで描く場合や、上書きしたいキー（色など）を調べる場合に使う。v0.1.0 の公開後に追加したもので、未公開。
* スキルの基本の手順（`drawio-png-cli skill`）では、図形・アイコンの追加は参照の属性だけを案内し、`library style` の手順は載せない。手段を 1 つにして、エージェントが迷わないようにするため。参照を外す手順（`--keep-images` で取り出してから属性を消す）も案内する。
* `library style` の使い方は別の文書に分け、基本の手順からは「style のキーを調べる場合は読む」とだけ案内する。文書は版と一致させるため、`skills/drawio-png/` に置かず、`drawio-png-cli skill` と同じく CLI に同梱して出力する。

#### draw.io での保持の確認

`<mxCell>` と `<object>` に独自の属性を付けた `.drawio.png` を、VS Code 拡張（draw.io 26.0.2）と draw.io Desktop 31.4.5（Linux arm64）で編集・保存し、`extract` で確認した（2026-10-11）。VS Code は手作業、Desktop は DevTools Protocol から draw.io の操作（`labelChanged`・`moveCells`・`copy`／`paste`・`duplicate`・`setStyle`・`setCellStyles`・`setAttributeForCell`）を呼び、保存（`save` アクション）した。レビュー（2026-10-11）で、draw.io 26.0.2 の Web 資材を使い codec と `Graph` の API でも確かめた。

| 操作 | VS Code 拡張 | Desktop |
| --- | --- | --- |
| 開いて保存（無関係な図形を追加） | 保持 | 保持 |
| ラベルの変更 | 保持 | 保持 |
| 移動（接続線を含む） | 保持 | 保持 |
| コピー＆ペースト | 新しいセルにも複製 | 新しいセルにも複製 |
| 複製（`duplicate`） | 新しいセルにも複製（API で確認） | 新しいセルにも複製 |
| スタイルの編集（style 全体の変更・キー単位の変更） | 保持 | 保持 |
| スタイルの貼り付け | style の独自のキーは写らない（`Graph.cellStyles` のキーだけを貼り付けるため。API で確認） | 未確認 |
| データを編集（`setAttributeForCell`） | セルが `<UserObject>` で包まれ、`<mxCell>` の属性は中の `<mxCell>` に残る（API で確認） | 同左 |

* `;`・`=`・日本語・`&`・`"`・`true`・空文字を含む値は、そのまま保持された。
* `<mxCell>` の属性のうち、数値に見える値は数値として読み込まれ、正規の表記で書き戻される（`007` → `7`、`1.50` → `1.5`、`1e3` → `1000`、` 42` → `42`）。`mxObjectCodec.isNumericAttribute` が `mxUtils.isNumeric` で判定し、`mxCellCodec` が除外するのは `value` だけのため。`<object>` の属性は変換されない。
* 保持されるのは、draw.io の `mxObjectCodec` が未知の XML 属性をセルのプロパティとして読み込み、保存時にすべてのプロパティを属性として書き出すことによる（`decodeAttribute`・`encodeObject`）。`<mxCell>` の属性は公式の独自データの置き場所ではないため、将来の draw.io で変わる可能性がある。往復する参照を `<object>` に置くのはこのためでもある。

#### 未決定事項

* `library style` の使い方を出力するコマンドの形（例: `drawio-png-cli skill --topic library-style`）。

---

## 5. 自動バリデーション仕様 (Validation Rules)

ファイル破損を防ぐため、`drawio-png-cli embed` 実行時および `drawio-png-cli validate` 呼び出し時には以下の **3段階の検証** を直列で実施する。いずれかの段階でエラーが発生した場合、処理を即座に中断し、終了コード `1` とエラー内容を出力する。

```
[入力 XML/stdin]
       │
       ▼
1. XML Syntax Check (Well-formed)
       │ Pass
       ▼
2. Root Tag Check (<mxfile> / <mxGraphModel>)
       │ Pass
       ▼
3. Essential Cell Check (id="0" & id="1")
       │ Pass
       ▼
 [処理継続 (embed / exit 0)]

```

1. **XML 構文チェック (Well-formed Check)**:
* タグの対構造、エスケープ漏れ、文字コードが正常な XML であるか。


2. **ルート要素チェック (Root Tag Check)**:
* ルート要素が `<mxfile>` または `<mxGraphModel>` であるか。


3. **基盤セル存在チェック (Essential Cell Check)**:
* 各ページの `<mxGraphModel><root>` 直下にデフォルトセル（`id="0"` および `id="1"` の `<mxCell>`）が存在するか。圧縮ページは展開して検証し、空の `<mxfile>` や復号不能なページは拒否する。



---

## 6. レンダリング戦略

draw.io Desktop 27.0.2 以降と Chromium / Chrome のヘッドレス描画に対応する。Desktopの先頭ページは 1-based の `--page-index 1`、Web描画APIは `from: 0, to: 0` で指定する。

* Desktop の検索順: `DRAWIO_PNG_DRAWIO_PATH` → PATH の `drawio` / `draw.io` → OS 標準インストール先。
* `DRAWIO_PNG_DRAWIO_PATH` の明示指定が不正な場合はエラーとし、別の候補には切り替えない。
* `DRAWIO_PNG_DRAWIO_ARGS` で Desktop の追加オプションを指定できる。`DRAWIO_PNG_DRAWIO_PATH` は実行ファイルのパスのみを受け付ける。
* `DRAWIO_PNG_DRAWIO_ARGS` は全 OS 共通の POSIX シェル形式で引用符・バックスラッシュを処理して引数へ分割する。シェル実行、環境変数展開、コマンド置換、ホームディレクトリ展開、glob 展開は行わない。
* 未設定・空文字・空白のみは追加引数なし。閉じていない引用符または Unicode として読めない環境変数は Desktop 起動前に終了コード `1` でエラーにし、既存出力を維持する。
* 追加引数は `drawio-png-cli` が生成する描画引数の前に渡す。入力ファイル・出力先・形式・ページ選択は `drawio-png-cli` が管理し、これらを変更する追加オプションや `--` の指定はサポートしない。
* `extract`・`validate`・`embed --no-render` は `DRAWIO_PNG_DRAWIO_ARGS` を読み取らない。
* 一時ファイルへ先頭ページを描画し、全ページの XML は `drawio-png-cli` が PNG に埋め込む。
* 描画は 60 秒でタイムアウトする。選択可能なレンダラーがない場合・描画失敗時はエラー終了する。
* `--no-render` では Desktop を検索・起動せず、ベース画像または新規透明 PNG にメタデータを保存する。
* Linux のディスプレイがない環境では利用者が Xvfb 等を用意する。`drawio-png-cli` は sandbox を自動無効化しない。
* `embed --renderer auto|desktop|chromium` を提供し、既定は `auto`。Desktopが見つからないときだけChromiumへ切り替え、明示指定の不正や描画失敗では切り替えない。
* Chromiumの検索順は `DRAWIO_PNG_CHROME_PATH` → `CHROME_PATH` → PATH → OS標準パス。PATH・OS標準パスでは Chrome を Chromium より優先する。`DRAWIO_PNG_CHROME_ARGS` はPOSIX形式で分割し、シェルを起動しない。
* Chromiumは基本描画資材をバイナリに同梱し、`DRAWIO_PNG_DRAWIO_WEB_PATH` で `export3.html` を含むローカルWeb資材を指定できる。不正指定ではエラーとし、資材の自動取得はしない。
* 同梱対象は基本図形・接続線・日本語・HTMLラベル・埋め込み画像。追加アイコン・ステンシル、数式・Mermaid・自動レイアウト資材は除外。必要資材が欠ける図面や未知の図形ではエラーにする。
* 外部HTTP(S)画像・フォント等の取得は既定で禁止。Chromium専用の `--allow-network` で許可できる。`--renderer` / `--allow-network` と `--no-render` は引数エラー。
* RustからCDPで専用プロファイルのChromiumを制御し、ループバックの資材サーバーを使用する。起動からPNG取得まで60秒。タイムアウト・失敗時もブラウザーとサーバーを片付け、既存出力を維持する。
* Chromiumは `--chromium-mode raw|desktop|vscode` で出力方式を選ぶ。CLI指定 → `DRAWIO_PNG_CHROMIUM_MODE` → `vscode`（SVG→Canvas、XMLのscale・border反映）の順に選ぶ。環境変数はChromium選択時だけ参照し、CLI指定があれば無視する。`raw` はDPR 1のキャプチャを画素加工せず使用し、`desktop` はDPR 2のキャプチャをHamming1で半分に縮小する。全モードで全ページXMLを埋め込む。モード指定はno-render・Desktop選択時には拒否する。取得画像・中間SVG／CanvasはRGBAで64 MiB以下に制限し、60秒の描画期限を維持する。詳細は [描画互換性](rendering.md) に記載する。
* 資材のライセンス・出典・ハッシュを記録する。`drawio-png-cli licenses` で同梱資材の表示・全文を取得可能とする。
* `embed --default-font FAMILY` と繰り返し指定可能な `--fallback-font FAMILY` により、両レンダラーへ渡すセルのフォント候補を統一できる。既存の明示指定を優先し、設定を全ページの埋め込みXMLにも保存する。省略時は従来の動作を保持し、特定の日本語フォントを強制しない。適用範囲と互換性は [フォント指定の仕様](fonts.md) に従う。

コンテナでの指定例（Desktop・Xvfb・xauth のインストールが必要）:

```sh
export DRAWIO_PNG_DRAWIO_PATH=/opt/drawio/drawio
export DRAWIO_PNG_DRAWIO_ARGS='--disable-gpu --disable-dev-shm-usage'
xvfb-run -a drawio-png-cli embed -i diagram.xml -o diagram.drawio.png
```

追加オプションは仮想ディスプレイを起動しない。Xvfb を `drawio-png-cli` の外側で起動するか、`DRAWIO_PNG_DRAWIO_PATH` に `"$@"` を転送するラッパーを指定する。

---

## 7. 非機能要件 & 技術スタック (Non-functional Requirements)

### 7.1. 非機能要件

* **高速起動**: AI エージェントのツール呼び出しループを阻害しないため、リリースビルドの起動と小規模入力の処理を数十ミリ秒以下の目標で測定する。外部レンダラーの起動・描画時間は別途評価する。
* **ポータビリティ**: 外部依存（Node.js や Python ランタイム等）を持たない単一バイナリ（Single Binary）として配布可能であること。
* **アトミック書き込み**: 出力先と同じディレクトリの一時ファイルに保存・同期後、アトミックに置換する。検証・描画・保存失敗時は既存出力を維持する。

### 7.2. 技術スタック (Rust)

* **言語**: Rust
* **使用クレート**:
* `clap`: CLI 引数・サブコマンドの構文解析
* `png`: PNG 画像の検査と透明画像の生成。メタデータはチャンク単位で直接置換し、`crc32fast` で CRC を検証・生成する
* `flate2`: draw.io データの Deflate 圧縮・解凍
* `base64`, `urlencoding`: メタデータのエンコード・デコード処理
* `roxmltree`: XML の検証とソース範囲に基づくページ展開。全体の再シリアライズを避け、未知要素や属性を保持する
* `tempfile`: 同一ディレクトリでのアトミック保存と描画用一時ファイル
* `shell-words`: `DRAWIO_PNG_DRAWIO_ARGS` の引用符・エスケープを含む引数分割

---

### 7.3. 入出力の制約

* UTF-8 XML（BOM 可）を扱い、DTD・外部エンティティは拒否する。
* 入力・展開データ・出力 PNG・デコード後の画像バッファはそれぞれ 64 MiB を上限とする。
* PNG シグネチャ、チャンク境界、CRC、画像データを検査する。
* `tEXt` / `zTXt` の `mxfile` / `mxGraphModel` を読み取る。旧 raw DEFLATE と二重 URL エンコードにも対応する。
* 競合する図面メタデータはエラー。同一内容の重複は許容する。書き込み時は図面メタデータを一つに統一する。
* 成功 `0`、処理・検証エラー `1`、引数エラー `2`。診断・警告は stderr に出力する。

## 8. 将来の拡張性 (Future Scope)

* **GitHub Actions 対応**: リポジトリ内の `.drawio.png` が常に正しい状態であることを自動チェックする CI アクションの提供。
* **Diff サブコマンド (`drawio-png-cli diff`)**: 2つの `.drawio.png` の XML 構造差分（追加・削除された図形やテキスト）をテキストベースで出力する機能。
