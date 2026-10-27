# レンダラーと描画互換性

`d-png-cli write --renderer vscode|desktop-compat|raw|desktop-app` で描画方式を選ぶ。CLI指定がなければ `D_PNG_RENDERER`、プロジェクトの `config.toml`、ユーザーの `config.toml` の `[render] renderer` の順に読み、いずれも未指定なら `vscode`。指定したレンダラーが見つからない・描画に失敗した場合に、別の方式へは切り替えない。`vscode`・`desktop-compat`・`raw` はChromiumで描画し、`desktop-app` はDesktopアプリのCLIを呼ぶ。

`D_PNG_RENDERER` と `renderer` の値は小文字の `vscode`・`desktop-compat`・`raw`・`desktop-app` の完全一致とし、空文字・未知値・非Unicode値は描画前に拒否する。描画しない処理（`read`・`expand`・`validate`・`write --metadata-only`）では参照しない。v0.1.0 の `--renderer auto|desktop|chromium`・`--chromium-mode`・`DIP_CHROMIUM_MODE` はこの指定にまとめた（Chromiumの `raw`→`raw`、`desktop`→`desktop-compat`、`vscode`→`vscode`、Desktopアプリ→`desktop-app`）。

| レンダラー | 描画・PNG取得 | 100×40矩形の検証寸法 |
| --- | --- | --- |
| `raw` | draw.ioエクスポーターをDPR 1で描画し、画面キャプチャをそのまま使用 | 103×43 |
| `desktop-compat` | DPR 2の画面キャプチャをHamming1で半分に縮小 | 104×44 |
| `vscode`（既定） | VS Code拡張1.9.0と同じdraw.io 26.0.2の `Editor.exportToCanvas()` でSVGを画像化しCanvasからPNGを取得 | 101×41 |

Chromiumの3方式とも指定ページ（既定は先頭）を描画し、全ページの編集用XMLを埋め込む。`raw` の「そのまま」はPNGの画素を縮小・再加工しないという意味で、XMLメタデータは追加する。

`--renderer desktop-app` はDesktopアプリのCLIを呼ぶ。一方、`--renderer desktop-compat` はDesktopアプリなしで互換処理を行う。

## raw・desktop-compatの画面キャプチャ

draw.ioの `render()` に `format:png, scale:1, border:0, theme:light` を渡し、先頭ページと資材の読み込みを待つ。XMLの `scale`・`border` はこの2方式では使わない。

`raw` は `ceil(bounds.size + bounds.offset)` の寸法でDPR 1のPNGを取得する。RustでPNGを検証し、画像データを再エンコードせずXMLを埋め込む。

`desktop-compat` は上記寸法に縦横各1pxを追加し、DPR 2で取得してHamming1で半分に縮小する。Desktop 31.4.5 / Electron 44.2.0 / Linux ARM64 / Xvfbの1倍表示を参照した処理であり、文字なし矩形のRGBA一致を固定fixtureで確認する。文字・フォント・ブラウザー・Desktop側の表示倍率によって差は残る。

Hamming1処理はChromiumのフィルター係数・固定小数点丸めに合わせたRust移植で、半透明と画像端も独立したElectron生成fixtureで検証する。このコードはBSD-3-Clauseのため、レンダラーの選択にかかわらず配布物にはライセンス表示を保持する。

rawの取得画像はRGBAで64 MiB（16,777,216画素）以下。desktop-compatは2倍取得にも64 MiB制限を適用するため、最終画像は4,194,304画素以下。上限は取得前に検査し、desktop-compatの縮小・PNG再エンコードも描画期限内で行う。

## vscodeの比較対象

基準はVS Code拡張 `hediet.vscode-drawio` の安定版1.9.0（タグ `v1.9.0`、`132921f1c189d1b6239e2b0246ebd8190ca23ba5`、2025-02-19）。同梱するvscode用Web資材は、このタグが固定するdraw.ioサブモジュール `96a916a337d13fc8bf622c8a67d422bd284eabe5`（26.0.2）である。raw・desktop-compat用の資材（31.4.5）とは別に同梱する。

拡張の2026-06以降のプレリリース（`v1.9.2606…` 以降のタグ）は31.x系のdraw.ioを固定しているが、`package.json` の版数は1.9.0のままである。draw.io 31.x では線幅の描画位置に合わせて出力範囲を広げる処理（jgraph/drawio#4938）が加わり、100×40の矩形は102×42になる。安定版1.9.0では101×41で、d-png-cliの `vscode` レンダラーはこちらに合わせる。以前の基準（31.4.5）での比較は [調査記録](vscode-rendering.md) に残している。

拡張v1.9.0のWebview HTMLとメッセージ送受信をGoogle Chrome 154.0.8037.97 / Linux ARM64上で再現し、エディターの表示倍率100%で保存したPNGを基準にする。VS Code本体での実機検証ではない。拡張側はlightテーマ、`simpleLabels=false`、追加プラグイン・カスタムスタイルなし。

### VS Code本体の保存結果との差

- 拡張の保存結果は、保存時のエディターの表示倍率に依存する。倍率100%では一致するが、ウィンドウに合わせて縮小表示していると線の位置がずれる。d-png-cliは倍率100%の結果に合わせる。
- 画面の拡大率（DPR 1・1.25・1.5・1.75・2）は結果に影響しない。
- devcontainer・WSLを使う場合も、Webviewはホスト側のVS Codeが描画する。WindowsとLinuxでは文字（Helveticaの代替フォント）、点線・アイコンの丸めが異なり、寸法・配置が一致しても画素差が残る。
- OS・フォント・ブラウザー・Web資材・拡張設定が異なる場合の完全一致は保証しない。

Desktop互換の方式選定時の実測詳細はコミット `24dd930` の `docs/rendering.md` に保存されている。

## vscodeの描画と保存

1. 参照の展開を含めて全ページを検証・展開し、描画するページ（既定は先頭）の `mxGraphModel` を描画する。ページ間のセルID重複が干渉しないよう、描画用には先頭ページを独立したXML文書へ移す。
2. draw.ioの `Editor.setGraphXml()` で背景・図形・ラベル等を読み込む。フォントの読み込み完了を待って再描画する。
   ラベルの `%page%`・`%pagenumber%`・`%pagecount%` は、拡張の `EditorUi` と同じくページ名・1・全ページ数に置換する。`mxfile` で囲まれていない単独の `mxGraphModel` のページ名は、拡張が英語UIで作るページと同じ `Page-1` とする。draw.io 26.0.2 は `%pagecount+1%` などの計算式を解釈しないため、そのまま表示する。`raw`・`desktop-compat` は上流 `export3.html` の処理に従い、Desktopと同様に `%pagecount%` を置換しない。
3. `mxfile` の `scale`（既定1）・`border`（既定0）を `exportToCanvas()` に渡す。倍率と余白の適用順もdraw.ioに従う。
4. SVG内の画像・フォントを埋め込み、Canvasの `toDataURL('image/png')` からPNGを取得する。
5. RustでPNGを検証し、全ページのXMLを埋め込んでアトミックに保存する。XMLはCLIの正規化結果を使い、拡張が再保存するXML文字列との完全一致は求めない。

`scale`・`border` は拡張同様に `parseFloat` で読み取り、未指定・数値として読めない値は既定値を使う。0以下の倍率、負の余白、無限大はエラーにする。背景未指定・`none` は透明、明示された背景色はCanvasにも反映する。DPRは出力倍率に用いない。

## vscodeの容量と時間の制限

中間SVG画像と最終Canvasの寸法を画像化前に検査し、それぞれ一辺16,384px以下、RGBA換算64 MiB（16,777,216画素）以下に制限する。倍率を小さくしても中間SVGが上限を超えれば拒否する。極端な縦長・横長や巨大な倍率も拒否する。

VS Code側の大画像に対する自動縮小は適用しない。CLIは上限超過をエラーにし、既存の出力を保持する。入力・取得PNG・ブラウザーの資材転送にも既存の64 MiB制限を適用し、JavaScript実行、PNG取得・検証を含めて60秒でタイムアウトする。

## 外部資材

外部画像・Webフォントのネットワーク取得は既定で禁止する。`vscode` では `--allow-network` 使用時も、Canvasへ埋め込むには配信元のCORS許可が必要。draw.ioの公開画像プロキシへは転送しない。取得・デコードに失敗した資材を検出した場合はエラーにする。

同梱資材で扱えない図形等には `D_PNG_DRAWIO_WEB_PATH` を利用できる。`vscode` では、拡張と同じく指定先の `js/shapes-14-6-5.min.js`・`js/stencils.min.js` があれば描画前に読み込み、`shapes/*.js` は取得しない。draw.io 26.0.2には `shapes/` がなく、AWS等の図形はこれらのファイルから読み込む。図形・アイコン（AWS図形の `resIcon`・`grIcon`、ステンシルの `include-shape` 等）が見つからない場合、上流は空白のまま描画を続けるが、d-png-cliはエラーにして既存出力を保持する。ただし、draw.io 26.0.2のサイドバー自身が書き出すのにどの版にも登録されていない図形名（`rect`・`text`・`mxgraph.android.rect` など8種類）は、セルの図形名に指定された場合に限り、上流と同じく既定の四角形として描画する。同じ名前をアイコン（`resIcon`・`grIcon` 等）に指定した場合は、上流が何も描かないため、従来どおりエラーにする。指定先の `export3.html` が、`vscode` では `Graph`・`Editor`・`Editor.exportToCanvas()`、`raw`・`desktop-compat` では `render()`・`LoadingComplete` を提供する必要がある。確認済みの資材は、vscodeが `96a916a337d13fc8bf622c8a67d422bd284eabe5`（26.0.2）、raw・desktop-compatが `f3abfe0f082c18f7b4fee8a34c2d07b1987687fd`（31.4.5）。別コミットの資材は互換性の検証対象外。

## 回帰テスト

vscodeの文字なし矩形と `scale=2, border=10` のPNGは、拡張v1.9.0の保存経路で独立生成した固定fixtureとRGBAを比較する。Desktop互換はDesktop基準fixtureと比較し、rawは直接取得の寸法とXML保持を確認する。複数ページの選択、透明背景・明示背景、HTML・埋め込み画像、外部画像の許可／禁止、容量超過、不正倍率、タイムアウトもChromium実機テストで確認する。

```sh
D_PNG_TEST_CHROME_PATH=/usr/bin/google-chrome \
  D_PNG_TEST_CHROME_ARGS=--disable-dev-shm-usage \
  mise exec -- cargo test --locked --test chromium -- --ignored --nocapture --test-threads=1
```
