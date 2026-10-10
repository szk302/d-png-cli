#!/usr/bin/env python3
"""Regenerate the draw.io shape catalog from a local upstream checkout (maintainer only).

Usage: shape_catalog.py /path/to/drawio-checkout

Runs the sidebar of the pinned vscode-mode draw.io (26.0.2) in headless Chrome
(DRAWIO_PNG_CHROME_PATH, CHROME_PATH, or google-chrome/chromium on PATH) and writes
assets/shapes/catalog.json.gz, which drawio-png-cli embeds as the built-in `drawio/*`
libraries of `drawio-png-cli library` and `drawio-png-cli insert`.

Entries whose cells embed image data (`data:image/...`) are left out, so the
catalog carries only cells, styles and references, never icon artwork.
"""
import gzip
import html
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
REVISION = '96a916a337d13fc8bf622c8a67d422bd284eabe5'  # draw.io 26.0.2, as in vendor_drawio.py
BUNDLE = ROOT / 'assets/shapes/catalog.json.gz'
# Stencils that sidebar entries name but the pinned revision does not define.
UPSTREAM_MISSING = ('mxgraph.aws4.piop',)


def chrome():
    for name in ('DRAWIO_PNG_CHROME_PATH', 'CHROME_PATH'):
        if os.environ.get(name):
            return os.environ[name]
    for name in ('google-chrome', 'chromium', 'chromium-browser'):
        if shutil.which(name):
            return shutil.which(name)
    sys.exit('Chrome not found; set DRAWIO_PNG_CHROME_PATH')


def palettes(checkout):
    subprocess.check_call(['git', '-C', str(checkout), 'cat-file', '-e', REVISION + '^{commit}'])
    with tempfile.TemporaryDirectory() as work:
        # Read the pinned Git objects, never uncommitted checkout contents.
        archive = subprocess.check_output(
            ['git', '-C', str(checkout), 'archive', REVISION, 'src/main/webapp'])
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(work, filter='data')
        webapp = Path(work, 'src/main/webapp')
        shutil.copy(Path(__file__).with_name('shape_catalog.html'), webapp / 'catalog.html')
        dom = subprocess.run(
            [chrome(), '--headless=new', '--disable-gpu', '--no-sandbox', '--no-first-run',
             '--disable-background-networking', '--disable-component-update', '--disable-sync',
             '--user-data-dir=' + str(Path(work, 'profile')), '--allow-file-access-from-files',
             '--dump-dom', (webapp / 'catalog.html').as_uri()],
            check=True, capture_output=True, text=True, timeout=300).stdout
    match = re.search(r'<pre id="out">(.*?)</pre>', dom, re.DOTALL)
    if not match or not match.group(1):
        sys.exit('the catalog page produced no output')
    result = json.loads(html.unescape(match.group(1)))
    if result['errors']:
        sys.exit('the sidebar failed:\n' + '\n'.join(result['errors']))
    return result['palettes']


def missing_images(checkout):
    """Image paths styles may name (img/...) that the pinned revision lacks."""
    files = subprocess.check_output(
        ['git', '-C', str(checkout), 'ls-tree', '-r', '--name-only', REVISION, 'src/main/webapp/img'],
        text=True).split('\n')
    present = {f.removeprefix('src/main/webapp/') for f in files if f}
    return lambda path: path.startswith('img/') and path not in present


def slug(palette_id):
    text = re.sub(r'(?<=[a-z0-9])(?=[A-Z])', '-', palette_id)
    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')


def text(value):
    return re.sub(r'\s+', ' ', value or '').strip()


def title_of(entry):
    """The sidebar title, else the first cell's label, else the shape or image name."""
    if text(entry['title']):
        return text(entry['title'])
    label = re.match(r'<(?:mxCell|object|UserObject)\b[^>]*?\s(?:value|label)="([^"]*)"', entry['xml'])
    if label:
        plain = text(re.sub(r'<[^>]*>', ' ', html.unescape(html.unescape(label.group(1)))))
        if plain:
            return plain if len(plain) <= 60 else plain[:59].rstrip() + '…'
    style = re.search(r'style="([^"]*)"', entry['xml'])
    name = re.search(r'(?:^|;)(?:image|shape)=([^;]+)', html.unescape(style.group(1))) if style else None
    if name:
        last = re.split(r'[/.]', re.sub(r'(_\d+x\d+)?\.(png|svg|jpe?g|gif)$', '', name.group(1)))[-1]
        return text(last.replace('_', ' '))
    return ''


def size(value):
    return value if isinstance(value, (int, float)) and value > 0 else 0


def main():
    if len(sys.argv) != 2:
        sys.exit('Usage: shape_catalog.py /path/to/drawio-checkout')
    checkout = Path(sys.argv[1])
    missing = missing_images(checkout)
    catalog, seen = [], set()
    for palette in palettes(checkout):
        # A palette built without its family's arguments has broken styles and sizes.
        broken = [e for e in palette['entries'] if re.search(r'undefined|="NaN"', e['xml'])]
        if broken:
            sys.exit(f'{palette["id"]} was built without its arguments')
        entries = []
        for e in palette['entries']:
            if 'data:image' in e['xml']:
                continue  # icon artwork is never redistributed
            # Per cell, as draw.io reads a style: the last image= wins.
            images = [found[-1] for style in re.findall(r'style="([^"]*)"', e['xml'])
                      if (found := re.findall(r'(?:^|;)image=([^;]*)', html.unescape(style)))]
            if any(map(missing, images)) or any(n in e['xml'] for n in UPSTREAM_MISSING):
                print(f'note: left out {palette["id"]} {e["title"]!r}: upstream lacks its image or stencil',
                      file=sys.stderr)
                continue
            entries.append({'title': title_of(e), 'w': size(e.get('w')), 'h': size(e.get('h')),
                            'xml': e['xml']})
        if not entries:
            continue
        name = slug(palette['id'])
        assert name not in seen, name
        seen.add(name)
        catalog.append({'name': name, 'title': text(palette['title']), 'entries': entries})
    # Name order, independent of the order the sidebar functions ran in.
    catalog.sort(key=lambda p: p['name'])
    names = {p['name'] for p in catalog}
    for required in ('aws4-compute', 'aws4-groups', 'azure2-compute', 'gcp2-zones', 'kubernetes',
                     'general', 'arrows', 'cisco-routers', 'signs-safety', 'rack-cisco', 'pid-pumps'):
        assert required in names, required
    data = json.dumps({'revision': REVISION, 'palettes': catalog},
                      ensure_ascii=False, separators=(',', ':'))
    BUNDLE.parent.mkdir(exist_ok=True)
    BUNDLE.write_bytes(gzip.compress(data.encode(), compresslevel=9, mtime=0))
    print(f'{len(catalog)} palettes, {sum(len(p["entries"]) for p in catalog)} shapes, '
          f'{BUNDLE.stat().st_size} bytes')


if __name__ == '__main__':
    main()
