# VS Code draw.io拡張とのPNG互換性（変更前の調査）

以下はSVG→Canvasへの変更前（コミット `24dd930`）の記録。現在の方式は [rendering.md](rendering.md) を参照。コマンド・オプション名は当時（v0.1.0）のもので（実行ファイル名は現在の `d-png-cli` で記す）、`--renderer desktop` は v0.2.0 の `desktop-app`、`--renderer chromium` の Desktop 互換処理は `desktop-compat` に当たる。

注: ここで調査した拡張のコミット `79500e6` はプレリリース相当で、draw.io 31.4.5を固定している。安定版1.9.0（タグ `v1.9.0`）はdraw.io 26.0.2を固定しており、現在のvscodeモードはこちらに合わせている。

2026-09-20に確認した範囲では、VS Code拡張のPNG保存結果と、`d-png-cli`のDesktop／Chromium出力は一致しない。文字なし矩形でも寸法と画素が異なる。前回のDesktop互換調整は、VS Code拡張との一致を意味しない。

## 対象と検証方法

- 拡張: `.tmp/vscode-drawio`、`hediet.vscode-drawio`、package.jsonのバージョン1.9.0、コミット `79500e6d467a95906a5f03680627c8f26ad3a0af`。
- 拡張のdrawioサブモジュール: `f3abfe0f082c18f7b4fee8a34c2d07b1987687fd`。起動したエディターの表示バージョンは31.4.5。
- CLI同梱draw.io資材: `744cb5420fdf126efd7a09b1d7082ca3e12c0841`。
- Desktop: 31.4.5 / Electron 44.2.0、Xvfbの1倍表示。
- Chromium: 153.0.8010.47、Linux ARM64（Debian 13）。

VS Code本体で拡張を起動した検証ではない。拡張のオフライン用 `src/DrawioClient/webview-content.html` をChromiumで実行し、`acquireVsCodeApi()` のメッセージ送受信をテスト用に置き換えた。HTMLが読み込む `app.min.js` 等は、サブモジュールの固定コミットをローカルGitオブジェクトから展開して提供した。

lightテーマ、`simpleLabels=false`、空のlocalStorage、カスタムスタイル・追加フォント設定なしで、拡張と同じ `configure` → `load` → `{action:"export",format:"xmlpng"}` を送った。UI操作・共同編集等の拡張独自プラグインは読み込んでいない。VS Code固有のWebview制約、ユーザー設定、オンラインモード、macOS／Windowsは未検証。

比較対象はPNGファイルのバイト列ではなく、寸法とデコードしたRGBA。XMLメタデータや圧縮方式だけの違いを画素差として扱わない。CLIの両レンダラーも同じ入力から今回再出力した。

## 実測結果

「共通フォント」は `--default-font 'Noto Sans CJK JP' --fallback-font 'Noto Sans CJK JP'` で生成したXMLを、3つの経路すべてに入力した結果。

| 図面 | フォント | 拡張の保存経路 | d-png-cli Desktop | d-png-cli Chromium |
| --- | --- | --- | --- | --- |
| 100×40の文字なし矩形 | 指定なし／共通 | 102×42 | 104×44 | 104×44 |
| HTMLラベル | 指定なし／共通 | 242×82 | 244×84 | 244×84 |
| 日本語・英数字ラベル | 指定なし | 146×42 | 131×44 | 149×44 |
| 日本語・英数字ラベル | 共通 | 129×42 | 131×44 | 132×44 |
| 複数ページの先頭ページ | 指定なし | 146×42 | 131×44 | 149×44 |
| 複数ページの先頭ページ | 共通 | 129×42 | 131×44 | 132×44 |
| 文字なし矩形、mxfileにscale=2・border=10 | 指定なし | 244×124 | 104×44 | 104×44 |

フォントの組み合わせを別々に数えた9ケースすべてで、拡張とCLIの寸法が異なった。拡張経路をDPR 1と2で実行した結果は、9ケースとも寸法・RGBAが完全一致した。この保存経路の出力倍率はDPR 2だから2倍になるものではない。

余白だけの違いかを調べるため、文字なし矩形のCLI出力の上下左右を各1px除外し、102×42に揃えて比較した。Desktop・Chromiumとも、拡張出力と異なるRGBAが695画素／4,284画素残った。この診断は単純な余白除去では一致しないことを示すもので、一般的な位置合わせアルゴリズムの検証ではない。

DesktopとCLI Chromium間では、文字なし矩形のRGBA一致を再確認した。HTMLラベルには指定なしで994画素、共通フォントで761画素の差が残り、前回の結果と同じだった。

CLIが生成したPNGの再読み込みも確認した。9入力×2レンダラーの18ファイルを、拡張の `loadPngWithEmbeddedXml()` と同じdata URI形式で読み込み、`xmlpng` で保存し直した。全18件で、XMLを直接読み込んだ拡張出力と寸法・RGBAが一致した。再保存したPNGからの `d-png-cli extract`・`d-png-cli validate` も成功し、複数ページ入力の2ページが保持された。画像の完全一致はないが、今回の図面では読み込み・再保存の互換性は確認できた。

## 差が生じる処理

拡張の通常の `.drawio.png` 保存は、次の経路を通る。

1. `src/DrawioEditorProviderBinary.ts` の `saveAs()` が `drawioClient.export()` を呼ぶ。
2. `src/DrawioClient/DrawioClient.ts` の `exportAsPngWithEmbeddedXml()` が `xmlpng` を要求する。
3. `webview-content.html` が、mxfileの `scale`・`border` 属性があればメッセージへ追加する。
4. draw.ioの `EditorUi` が先頭ページを選び、`Editor.exportToCanvas()` でSVGを画像化してCanvasへ描き、`canvas.toDataURL('image/png')` でPNGを生成する。
5. 全ページのXMLをPNGの `tEXt` に埋め込む。

対して `d-png-cli --renderer chromium` は `export3.html` の描画結果をDPR 2でキャプチャし、Hamming1で半分に縮小する。呼び出し時に `scale:1, border:0` を指定する。`d-png-cli --renderer desktop` はDesktopのCLIエクスポートを使う。今回の `scale=2, border=10` の入力では、どちらのCLI経路も画像サイズにその属性を反映しなかった。

SVGの書き出しと画面キャプチャでは、切り出し範囲、描画位置、アンチエイリアス、文字の処理が異なる。また、拡張とCLIではdraw.io資材のコミットも異なる。共通フォントを指定しても、これらの違いはなくならない。

## 今後の互換性の基準

VS Code拡張で保存するPNGを基準にするなら、CLIにも同じSVG → Canvasの書き出し経路を用意し、`scale`・`border` の解釈とdraw.io資材を揃える必要がある。寸法計算や縮小フィルターだけの変更では足りない。Desktopと拡張自体の出力が異なるため、両方の既存出力へ同時に完全一致する単一の結果は作れない。

この調査ではCLIの描画実装は変更していない。既存のDesktop互換の仕様は [rendering.md](rendering.md) を参照。

## ローカルの検証資料

`.tmp/vscode-comparison/` に入力XML、全経路のPNG、検証スクリプト、集計を保存した（Git管理外）。

- `webapp/`: 固定したサブモジュールのWeb資材。
- `inputs/`: 比較に使用した9入力。
- `probe.mjs`: 拡張HTMLを実行し、DPR 1／2で出力する。
- `probe-results.json`: エディターのバージョン、描画範囲、実際のexportメッセージ等。
- `compare.py` / `results.json`: PNGをRGBAへデコードした寸法・一致判定。
- `roundtrip-results.json`: CLI生成PNGの読み込み・再保存18件の結果。

リポジトリルートで `mise exec -- node .tmp/vscode-comparison/probe.mjs`、`python3 .tmp/vscode-comparison/compare.py` を実行すると、準備済みのローカル資材で拡張側の出力と比較を再実行できる。Node.jsはこの調査用で、CLIの実行依存には追加していない。

`probe.mjs desktop` または `probe.mjs chromium` を指定すると、同ディレクトリに保存した各CLI出力を入力にして、DPR 1で再読み込み・再保存を実行する。
