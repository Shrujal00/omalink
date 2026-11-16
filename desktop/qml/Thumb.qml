import QtQuick
import omalink

// A file's picture: its thumbnail when there is one, otherwise a glyph for
// its kind on a tinted ground. The picture fades in once it has loaded.
Rectangle {
    id: thumb
    property string source: ""
    property string kind: "other"
    property real glyphSize: 30

    readonly property string glyph: ({ image: "image", video: "video", audio: "music", archive: "archive", apk: "package", doc: "doc" })[kind] || "file"
    readonly property color tint: ({ image: Theme.accent, video: Theme.red, audio: Theme.yellow, apk: Theme.green })[kind] || Theme.secondary
    readonly property bool loaded: picture.status === Image.Ready

    color: Qt.rgba(tint.r, tint.g, tint.b, 0.10)
    clip: true

    Icon { anchors.centerIn: parent; visible: !thumb.loaded; name: thumb.glyph; size: thumb.glyphSize; weight: 1.4; color: thumb.tint }
    Image {
        id: picture
        anchors.fill: parent
        source: thumb.source
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: false
        // Thumbnails are small already; never decode more than is shown.
        sourceSize.width: 320
        opacity: thumb.loaded ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.smooth } }
    }
    // A video says so over its picture.
    Rectangle {
        visible: thumb.kind === "video" && thumb.loaded
        anchors.centerIn: parent
        width: Math.min(44, parent.width * 0.4); height: width; radius: width / 2
        color: Qt.rgba(0, 0, 0, 0.55)
        border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.35)
        Icon { anchors.centerIn: parent; anchors.horizontalCenterOffset: 1; name: "play"; size: parent.width * 0.42; color: "white" }
    }
}
