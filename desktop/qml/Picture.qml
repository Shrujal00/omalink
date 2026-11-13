import QtQuick

// A picture cropped to fill a rounded shape. Draws nothing until it loads,
// so whatever sits underneath shows through. Painted on a canvas rather
// than masked with a shader, so it also works without a GPU. The canvas
// loads the file itself: drawing from another item's texture comes out
// blank on some renderers.
Item {
    id: picture
    property string source: ""
    property real radius: width / 2
    readonly property bool ready: probe.status === Image.Ready && canvas.loaded === source

    onRadiusChanged: canvas.requestPaint()
    onSourceChanged: canvas.load()

    // Only here to say how large the picture is.
    Image {
        id: probe
        source: picture.source
        asynchronous: true
        visible: false
        onStatusChanged: canvas.requestPaint()
    }
    Canvas {
        id: canvas
        anchors.fill: parent
        // The source whose pixels the canvas holds.
        property string loaded: ""
        function load() {
            if (loaded !== "" && loaded !== picture.source) unloadImage(loaded)
            loaded = ""
            if (picture.source !== "" && available) loadImage(picture.source)
            check()
        }
        // A file already loaded elsewhere in the app is ready at once and
        // announces nothing, so look as well as listen.
        function check() {
            if (picture.source !== "" && isImageLoaded(picture.source)) loaded = picture.source
            requestPaint()
        }
        onAvailableChanged: load()
        onImageLoaded: check()
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            if (!picture.ready || width <= 0 || height <= 0) return
            // Scaled to cover the item and centred; the clip trims the rest.
            const iw = probe.implicitWidth, ih = probe.implicitHeight
            if (iw <= 0 || ih <= 0) return
            const cover = Math.max(width / iw, height / ih)
            const r = Math.min(picture.radius, width / 2, height / 2)
            ctx.beginPath()
            ctx.roundedRect(0, 0, width, height, r, r)
            ctx.clip()
            ctx.drawImage(picture.source, (width - iw * cover) / 2, (height - ih * cover) / 2, iw * cover, ih * cover)
        }
    }
}
