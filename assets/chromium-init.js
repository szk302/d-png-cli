// Set local paths before upstream's viewer defaults can select public servers.
window.urlParams = {offline: '1', local: '1', dark: '0'};
window.PROXY_URL = null;
window.EXPORT_URL = null;
window.STYLE_PATH = 'styles';
window.SHAPES_PATH = 'shapes';
window.STENCIL_PATH = 'stencils';
window.DRAW_MATH_URL = 'math4/es5';
window.GRAPH_IMAGE_PATH = 'img';
window.mxImageBasePath = 'mxgraph/images';
window.mxBasePath = 'mxgraph/';
window.mxLoadStylesheets = false;
window.isLocalStorage = false;
window.drawioPngErrors = [];
window.addEventListener('error', event => {
    if (event.message) window.drawioPngErrors.push(event.message);
    else if (event.target?.tagName && event.target.tagName !== 'LINK') {
        window.drawioPngErrors.push('Unable to load ' + event.target.tagName + ' resource');
    }
}, true);
window.addEventListener('unhandledrejection', event => {
    window.drawioPngErrors.push(String(event.reason));
});
// Export preloads detached Image objects; their decode errors do not reach window.
window.Image = new Proxy(window.Image, {
    construct(target, args) {
        const image = Reflect.construct(target, args);
        image.addEventListener('error', () => window.drawioPngErrors.push('Image could not be decoded or loaded'));
        return image;
    }
});
document.fonts.addEventListener('loadingerror', () => {
    window.drawioPngErrors.push('Font could not be decoded or loaded');
});
// The exporter is a document renderer, not an interactive navigation surface.
window.open = () => null;
