// Called with JSON arguments through CDP, never XML interpolated as source code.
async function drawioPngRender(xml, bundled, mode, preload, pageIndex) {
    if (bundled) mxStencilRegistry.dynamicLoading = false;
    // Load the shape bundles the VS Code extension includes, in its order.
    for (const src of preload) {
        await new Promise((resolve, reject) => {
            const script = document.createElement('script');
            script.src = src;
            script.onload = resolve;
            script.onerror = () => reject(Error('Unable to load ' + src));
            document.head.appendChild(script);
        });
    }
    // shapes-14-6-5.min.js already defines every shapes/*.js library, and the
    // extension's draw.io has no shapes/ directory to load them from. Stencil XML
    // still loads on demand; stencils.min.js serves it from its embedded copies.
    if (preload.includes('js/shapes-14-6-5.min.js')) {
        for (const files of Object.values(mxStencilRegistry.libraries)) {
            for (const file of [].concat(files)) {
                if (typeof file === 'string' && file.startsWith(SHAPES_PATH + '/') && file.endsWith('.js')) {
                    mxStencilRegistry.setFileLoaded(file);
                }
            }
        }
    }
    const doc = mxUtils.parseXml(xml);
    if (doc.getElementsByTagName('parsererror').length) throw Error('Invalid XML');
    // pageIndex is 0-based; a bare mxGraphModel is page 0.
    const model = doc.documentElement.nodeName === 'mxGraphModel'
        ? (pageIndex === 0 ? doc.documentElement : null)
        : Array.from(doc.documentElement.children)
            .filter(n => n.nodeName === 'diagram')
            .map(d => Array.from(d.children).find(n => n.nodeName === 'mxGraphModel'))[pageIndex];
    if (!model) throw Error('Expected an uncompressed mxGraphModel on page ' + (pageIndex + 1));
    if (bundled && (model.getAttribute('math') === '1' ||
        model.querySelector('[layout]') || model.hasAttribute('layout'))) {
        throw Error('Math and automatic layout require local draw.io assets (DRAWIO_PNG_DRAWIO_WEB_PATH)');
    }
    const original = mxCellRenderer.prototype.createShape;
    // shape=stencil(...) embeds the stencil itself, so it is never a registry key.
    // Decode it like upstream Graph.decompress, but bound the inflated size and
    // reject broken data instead of silently drawing a default shape.
    const utf8 = chunks => {
        const bytes = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
        let offset = 0;
        for (const chunk of chunks) {
            bytes.set(chunk, offset);
            offset += chunk.length;
        }
        return new TextDecoder('utf-8', {fatal: true}).decode(bytes);
    };
    const stencils = new Map();
    let stencilBytes = 0;
    const inlineStencil = data => {
        if (stencils.has(data)) return stencils.get(data);
        let doc;
        try {
            const inflator = new pako.Inflate({raw: true});
            const chunks = [];
            inflator.onData = chunk => {
                stencilBytes += chunk.length;
                if (stencilBytes > 64 * 1024 * 1024) throw Error('exceeds 64 MiB limit');
                chunks.push(chunk);
            };
            inflator.push(Uint8Array.from(atob(data), c => c.charCodeAt(0)), true);
            if (inflator.err || !inflator.ended) throw Error(inflator.msg || 'truncated data');
            const text = utf8(chunks);
            doc = mxUtils.parseXml(Graph.zapGremlins(decodeURIComponent(text)));
        } catch (error) {
            throw Error('Invalid embedded stencil: ' + error.message);
        }
        if (doc.getElementsByTagName('parsererror').length || doc.documentElement.nodeName !== 'shape') {
            throw Error('Invalid embedded stencil: expected <shape>');
        }
        const stencil = new mxStencil(doc.documentElement);
        stencils.set(data, stencil);
        return stencil;
    };
    // Upstream silently skips an include-shape whose stencil is not loaded. Check
    // at draw time so nested references and registry stencils are covered too.
    const drawNode = mxStencil.prototype.drawNode;
    mxStencil.prototype.drawNode = function(canvas, shape, node) {
        if (node.nodeName === 'include-shape') {
            const name = node.getAttribute('name');
            if (!mxStencilRegistry.getStencil(name)) {
                throw Error('Unsupported shape: ' + name + ' (include-shape); provide assets with DRAWIO_PNG_DRAWIO_WEB_PATH');
            }
        }
        return drawNode.apply(this, arguments);
    };
    // draw.io 26.0.2's own sidebar writes these cell shape names, which no draw.io
    // version registers; upstream's createShape draws them as the default
    // rectangle, so drawio-png-cli does too. Only that lookup is exempt: an icon lookup
    // (resIcon, grIcon) with the same name draws nothing upstream and still fails.
    const sidebarFallbacks = new Set([
        'rect',
        'text',
        'mxgraph.android.rect',
        'mxgraph.gmdl.rect',
        'mxgraph.ios.rect',
        'mxgraph.archimate.rounded=1',
        'mxgraph.electrical.rot_mech.verticalLabelPosition=bottom',
        'mxgraph.pid.filters.liquid_Filter_(bag,_candle,_cartridge)',
    ]);
    // The fallback name createShape is looking up, while it does.
    let cellFallback = null;
    // Shapes such as mxgraph.aws4.resourceIcon look up their icon (resIcon, grIcon)
    // while painting and silently omit it when it is missing. Record every named
    // lookup that finds neither a stencil nor a shape class; drawio-png-cli fails on these.
    const getStencil = mxStencilRegistry.getStencil;
    mxStencilRegistry.getStencil = function(name) {
        const stencil = getStencil.apply(this, arguments);
        if (stencil == null && typeof name === 'string' && name.trim() !== '' &&
            !mxCellRenderer.defaultShapes[name] && name !== cellFallback) {
            window.drawioPngErrors.push('Unsupported shape: ' + name + '; provide assets with DRAWIO_PNG_DRAWIO_WEB_PATH');
        }
        return stencil;
    };
    mxCellRenderer.prototype.createShape = function(state) {
        const name = state.style[mxConstants.STYLE_SHAPE];
        if (typeof name === 'string' && name.startsWith('stencil(') && name.endsWith(')')) {
            return new mxShape(inlineStencil(name.slice(8, -1)));
        }
        if (sidebarFallbacks.has(name)) {
            cellFallback = name;
            try {
                return original.apply(this, arguments);
            } finally {
                cellFallback = null;
            }
        }
        // getStencil can load a shape library (e.g. shapes/mxAWS4.js) that registers
        // a shape class rather than a stencil, so check the registry again after it.
        if (name && !mxCellRenderer.defaultShapes[name] && !mxStencilRegistry.getStencil(name) &&
            !mxCellRenderer.defaultShapes[name]) {
            throw Error('Unsupported shape: ' + name + '; provide assets with DRAWIO_PNG_DRAWIO_WEB_PATH');
        }
        return original.apply(this, arguments);
    };
    if (mode !== 'vscode') {
        render({xml, format: 'png', from: pageIndex, to: pageIndex, scale: 1, border: 0, theme: 'light'});
        return;
    }
    const file = doc.documentElement;
    const option = (name, fallback) => {
        const value = file.nodeName === 'mxfile' ? Number.parseFloat(file.getAttribute(name)) : NaN;
        return Number.isNaN(value) ? fallback : value;
    };
    const scale = option('scale', 1), border = option('border', 0);
    if (!Number.isFinite(scale) || scale <= 0 || !Number.isFinite(border) || border < 0) {
        throw Error('Invalid PNG scale or border');
    }
    const container = document.createElement('div');
    document.body.appendChild(container);
    const graph = new Graph(container);
    const editor = new Editor(false, null, null, graph);
    graph.setEnabled(false);
    const page = model.parentNode;
    const getGlobalVariable = graph.getGlobalVariable;
    graph.getGlobalVariable = function(name) {
        // The extension wraps a bare mxGraphModel in a new page named Page-1 (English UI).
        if (name === 'page') return page.nodeName === 'diagram' ? page.getAttribute('name') : 'Page-1';
        if (name === 'pagenumber') return pageIndex + 1;
        // EditorUi normally supplies this; count pages before the first is separated.
        if (name === 'pagecount') return file.nodeName === 'mxfile'
            ? Array.from(file.children).filter(n => n.nodeName === 'diagram').length : 1;
        return getGlobalVariable.apply(this, arguments);
    };
    // mxCodec resolves IDs through the owner document. Separate the page so
    // identical cell IDs on other pages cannot affect this page's graph.
    editor.setGraphXml(mxUtils.parseXml(mxUtils.getXml(model)).documentElement);
    await document.fonts.ready;
    graph.refresh();
    // Guard both the intermediate SVG image and final Canvas before allocation.
    // Unlike the editor, the CLI rejects oversized exports rather than reducing scale.
    const checkSize = (w, h) => {
        if (!Number.isFinite(w) || !Number.isFinite(h) || w < 1 || h < 1 ||
            w > 16384 || h > 16384 || w * h * 4 > 64 * 1024 * 1024) {
            throw Error('Canvas image exceeds 64 MiB limit or maximum dimensions');
        }
    };
    const getSvg = graph.getSvg;
    graph.getSvg = function() {
        const svg = getSvg.apply(this, arguments);
        const w = parseFloat(svg.getAttribute('width')), h = parseFloat(svg.getAttribute('height'));
        checkSize(w, h);
        checkSize(Math.ceil(w * scale), Math.ceil(h * scale));
        return svg;
    };
    const canvas = await new Promise((resolve, reject) => {
        editor.exportToCanvas(resolve, null, null, null,
            error => reject(Error(error?.message || 'SVG to Canvas export failed')),
            // Resolve URLs directly; do not send images to draw.io's public proxy.
            null, null, scale, null, null, new mxUrlConverter(), graph, border);
    });
    if (window.drawioPngErrors.length) throw Error(window.drawioPngErrors.join('; '));
    checkSize(canvas.width, canvas.height);
    const uri = canvas.toDataURL('image/png');
    if (!uri.startsWith('data:image/png;base64,')) throw Error('Canvas returned no PNG');
    return uri.slice('data:image/png;base64,'.length);
}
