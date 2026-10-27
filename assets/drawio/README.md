# Bundled draw.io renderer

Upstream: https://github.com/jgraph/drawio

d-png-cli bundles two sets of the same four unmodified upstream files:

| Directory | Revision | draw.io | Used by |
| --- | --- | --- | --- |
| `.` | `f3abfe0f082c18f7b4fee8a34c2d07b1987687fd` | 31.4.5 | `raw` and `desktop-compat` renderers (Desktop 31.4.5 reference) |
| `vscode/` | `96a916a337d13fc8bf622c8a67d422bd284eabe5` | 26.0.2 | `vscode` renderer |

Each directory's `manifest.json` identifies the assets and SHA-256
hashes of their decompressed bytes. They are stored with deterministic gzip
headers and embedded into `d-png-cli`. `chromium.html`, `chromium-init.js`, and
`chromium-render.js` in the parent directory are original MIT-licensed d-png-cli
integration code; they configure local URLs, disable optional bundled math,
check unsupported shapes, decode embedded `shape=stencil(...)` shapes with a
size limit, and select `Editor.exportToCanvas()` for the `vscode`
renderer or `render()` for the `raw` and `desktop-compat` capture renderers.
The `vscode/` revision is the draw.io submodule pinned by the stable
[`hediet.vscode-drawio`](https://github.com/hediet/vscode-drawio) 1.9.0 release
(tag `v1.9.0`, `132921f1c189d1b6239e2b0246ebd8190ca23ba5`). The 31.4.5 revision
is pinned by the extension's later pre-releases, which also report version 1.9.0
in `package.json`. Its reference-only use is documented
in [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md). No upstream source is edited.

## Included works

- draw.io/mxGraph: Apache-2.0. Copyright JGraph Holdings Ltd and draw.io AG;
  individual years and notices remain in the upstream files.
- spin.js 2.0.0: MIT, Copyright (c) 2011-2014 Felix Gnass.
- DOMPurify 3.4.15 (31.4.5) and 3.2.3 (26.0.2): Apache-2.0 (selected from
  upstream's dual licensing), Copyright (c) Cure53 and other contributors.
- pako 2.2.0 (31.4.5) and 2.1.0 (26.0.2): MIT AND Zlib, Vitaly Puzrin,
  Andrei Tuputcyn/Tupitsin, Jean-loup Gailly and Mark Adler.
- Rough.js 4.6.6 and its hachure-fill, path-data-parser, points-on-path,
  points-on-curve components: MIT, Copyright (c) 2019/2020/2022 Preet Shihn.
  All upstream copyright statements are retained in `licenses/`.

Both revisions carry the same draw.io Apache-2.0 license text and the same
Rough.js and spin.js builds.
The viewer composition was checked against `etc/build/build.xml`: the nonstatic
viewer contains the basic mxGraph/editor/viewer code and the libraries above.
It does not concatenate `shapes-14-6-5.min.js`. This package excludes `app.min.js`,
`viewer-static.min.js`, `extensions.min.js`, the additional `shapes/`, `stencils/`,
`img/` packs, their compiled derivatives, templates, MathJax, Mermaid, ELK and
libavoid. References to optional asset paths in the viewer are not bundled packs.
The separately restricted icon/stencil packs are intentionally excluded; consult
upstream's `shapes/LICENSE`, `stencils/LICENSE`, and `img/LICENSE` before obtaining
those assets for use through `D_PNG_DRAWIO_WEB_PATH`. The upstream README at
the 31.4.5 revision also restricts the use of draw.io's icon sets and stencil
libraries in Atlassian products and the Atlassian marketplace; those packs are
not part of this bundle.

## License provenance

Full license texts in `licenses/` come from:

- draw.io: the pinned repository's `LICENSE`.
- spin.js: https://raw.githubusercontent.com/fgnass/spin.js/2.0.0/LICENSE.txt
- DOMPurify: https://raw.githubusercontent.com/cure53/DOMPurify/3.4.15/LICENSE
- pako: https://raw.githubusercontent.com/nodeca/pako/2.2.0/LICENSE and
  https://raw.githubusercontent.com/nodeca/pako/2.2.0/lib/zlib/README
- Rough.js and its components: the `LICENSE` files in npm source tarballs for
  roughjs 4.6.6, hachure-fill 0.5.2, path-data-parser 0.1.0,
  points-on-path 0.2.1 and points-on-curve 0.2.0. Each is also covered by the
  corresponding MIT copyright notices here; the minified viewer does not
  preserve the exact transitive dependency lockfile used by upstream.

`d-png-cli licenses` prints these notices and full texts, so standalone binary users
can read them. Source and binary archives must also retain `LICENSE`,
`THIRD_PARTY_NOTICES.md`, this file, `licenses/`, and `../licenses/` (Chromium
resampling and Cargo dependency licenses).

## Maintenance

Run `python3 scripts/vendor_drawio.py /path/to/drawio-checkout` to refresh from
the pinned Git objects, then review the composition and licenses. To change
versions, update the revision deliberately and repeat that review.
Run `python3 scripts/vendor_drawio.py --check` to verify stored hashes without
network access or an upstream checkout. Python is only used for maintenance
and integrity checks, not building or running d-png-cli.
