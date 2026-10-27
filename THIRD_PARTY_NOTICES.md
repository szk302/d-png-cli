# Third-party notices

This project's original code is licensed under the MIT License in [LICENSE](LICENSE).
Third-party works retain their own copyright notices and license terms.
This file documents bundled works and implementation references. Cargo dependency
notices and full license texts are in
[`assets/licenses/cargo-dependencies.txt`](assets/licenses/cargo-dependencies.txt).
That generated inventory covers all targets and includes build and development
dependencies; not every listed crate is linked into every binary. It selects
permitted license alternatives according to `about.toml` and retains combined
license requirements. `d-png-cli licenses` prints this file and the license texts.

## Trademarks

d-png-cli is not made or endorsed by draw.io Ltd. draw.io is a registered
trademark of draw.io Ltd. This project uses the name only to state that it
reads and writes draw.io diagrams and `.drawio.png` files.

## Chromium-compatible image resizing

The Chromium `desktop-compat` renderer uses `src/adapters/render/resample.rs`, which adapts the Hamming1 filter generation and fixed-point channel
rounding used by Chromium's `skia/ext/image_operations.cc` (Copyright 2012 The
Chromium Authors) and `skia/ext/convolver.{cc,h}` (Copyright 2011 The Chromium
Authors), reviewed at Chromium `152.0.7977.76`. The Rust implementation is limited
to 2:1 downsampling and uses bounded edge/interior kernel storage. It is licensed
under BSD-3-Clause; the full license is in
[`assets/licenses/chromium-BSD-3-Clause.txt`](assets/licenses/chromium-BSD-3-Clause.txt)
and is also printed by `d-png-cli licenses`.

The behavior reference is Electron `v44.2.0`
[`NativeImage::Resize`](https://github.com/electron/electron/blob/v44.2.0/shell/common/api/electron_api_native_image.cc),
whose `good` and `better` quality settings use Chromium's Hamming1 path. The
choice was verified against draw.io Desktop 31.4.5's actual capture output on a
1x Linux display; capture has already downsampled before Desktop calls resize.
No Electron code or runtime is bundled for this feature.

Retain this notice and the BSD license in source and binary distributions.

The BSD license text is copied from
[Chromium's root LICENSE at 152.0.7977.76](https://github.com/chromium/chromium/blob/152.0.7977.76/LICENSE).
Its 2015 copyright notice is retained verbatim; the 2011 and 2012 notices above
belong to the individual source files used by the resampling adaptation.

## drawio-exporter

- Project: https://github.com/rlespinasse/drawio-exporter
- Reviewed revision: `68aa3dfc52da4f70e0077aa569f6aa24d4a139d1`
- License: MIT
- Copyright: Copyright (c) 2021 Romain Lespinasse

The implementation consulted drawio-exporter for draw.io Desktop invocation,
standard executable locations, compressed-page decoding, and compatibility-test
organization. In particular, the references included
`src/drawio_exporter/core/drawio/drawio_desktop.rs`,
`src/drawio_exporter/core/drawio/mxfile.rs`, and its CLI tests.
These informed the separately written rendering, XML-processing, and test code
in this project. drawio-exporter is not a dependency and its source files and
fixtures are not bundled with d-png-cli.

The following upstream copyright and license text is retained in full as a
precautionary attribution for that implementation reference:

```text
MIT License

Copyright (c) 2021 Romain Lespinasse

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## VS Code Draw.io Integration (reference only)

- Project: https://github.com/hediet/vscode-drawio (`hediet.vscode-drawio`)
- Reviewed revisions: `79500e6d467a95906a5f03680627c8f26ad3a0af` and the
  stable release tag `v1.9.0` (`132921f1c189d1b6239e2b0246ebd8190ca23ba5`)
- Upstream license: GPL version 3 (`GPL-3.0` in its `package.json`);
  [license text at the reviewed revision](https://github.com/hediet/vscode-drawio/blob/79500e6d467a95906a5f03680627c8f26ad3a0af/LICENSE.md).

The extension was inspected to understand the message protocol and export options
in `src/DrawioClient/webview-content.html` and the `xmlpng` save path through
`src/DrawioClient/DrawioClient.ts` and `src/DrawioEditorProviderBinary.ts`.
d-png-cli's integration was written separately; extension source code is not copied
or bundled, and the extension is not a build or runtime dependency.

`tests/fixtures/geometry-vscode.png` and `geometry-options-vscode.png` are outputs
of that save path for this project's original rectangle diagrams, not copies of
extension code or artwork. A local test bridge ran the extension's Webview HTML
with its pinned draw.io assets to generate these fixtures. See
[fixture provenance](tests/fixtures/README.md). The GPL reference here describes
the inspected extension, not the license of d-png-cli or the independently authored
test diagrams. The separately bundled draw.io assets retain the licenses below.

## Other implementation references

- [drawio](https://github.com/jgraph/drawio), revision `744cb5420`:
  PNG metadata encoding/decoding and compressed diagram-page format.
  [Upstream license: Apache-2.0](https://github.com/jgraph/drawio/blob/744cb5420/LICENSE).
- [drawio-desktop](https://github.com/jgraph/drawio-desktop), revision `60ec92a`:
  Desktop export arguments and PNG export behavior.
  [Upstream license: Apache-2.0](https://github.com/jgraph/drawio-desktop/blob/60ec92a/LICENSE).

The local reference checkouts and Desktop test installation in `.tmp/` are not
included in this project's source package. d-png-cli calls a separately installed Desktop or Chromium executable.
The Chromium renderer bundles the limited subset described below. The test diagrams and PNG fixtures were generated for this project;
see [the fixture provenance](tests/fixtures/README.md).

## musl libc (Linux release binaries)

The Linux release archives are built for the `*-unknown-linux-musl` targets
of Rust 1.98.1, which statically link musl libc 1.2.5 (with the security
patches applied by the Rust toolchain). musl is MIT-licensed;
[its COPYRIGHT file](assets/licenses/musl-1.2.5-COPYRIGHT.txt) is included
unmodified from https://git.musl-libc.org/cgit/musl/plain/COPYRIGHT?h=v1.2.5
(SHA-256 `f9bc4423732350eb0b3f7ed7e91d530298476f8fec0c6c427a1c04ade22655af`).
These binaries also contain LLVM libunwind, whose Apache-2.0 WITH
LLVM-exception license does not require notices for such embedded object
code. The macOS and Windows binaries and builds for glibc targets do not
contain musl.

## Bundled Chromium renderer assets

The Chromium renderer embeds unmodified, gzip-compressed subsets of draw.io
at revisions `f3abfe0f082c18f7b4fee8a34c2d07b1987687fd` (31.4.5) and
`96a916a337d13fc8bf622c8a67d422bd284eabe5` (26.0.2). This differs from the
reference-only use of the Desktop checkout described above.

See [the asset inventory](assets/drawio/README.md),
[the source/hash manifest](assets/drawio/manifest.json), and
[full license texts](assets/drawio/licenses/). The bundle includes Apache-2.0,
MIT and Zlib works; those assets are not relicensed under d-png-cli's MIT license.
Separately restricted additional icon/stencil packs and their compiled bundles
are excluded. All retained notices and license texts are accessible in standalone
binaries through `d-png-cli licenses`.

Source and binary archives must retain this file, `LICENSE`,
`assets/drawio/README.md`, `assets/drawio/licenses/`, and `assets/licenses/`
(including the generated Cargo dependency notices). No runtime or build-time
asset download is performed.

## Shape libraries

d-png-cli bundles no shape or icon libraries. draw.io's sidebar shapes,
icon sets and stencil libraries are not included; users register the
libraries they choose (`library add`, `library install`) and are responsible
for the license and trademark terms of what they register. The libraries in
`docs/examples/` are original test material: their images are
plain colored rectangles, and the `shapes` example names draw.io's basic
shapes by style only.

## Rendering integration

The headless export integration calls draw.io's `Editor.exportToCanvas()` for
the `vscode` renderer, matching the extension's `xmlpng` save path. The `raw` and
`desktop-compat` renderers use draw.io's `render` / `LoadingComplete` capture protocol.
The VS Code extension reference is described above. Chromium and Electron runtimes
are not bundled. The Desktop-compatible resizing
code described above is included in d-png-cli.
