import QtQuick
import omalink

// A contact's picture where the phone has one; otherwise a circle with
// initials, tinted from the name so the same person always gets the same
// color. `name` is whatever the row shows: a contact name or a number.
Rectangle {
    id: avatar
    property string name: ""
    property real size: 38
    readonly property string photo: Store.photoFor(name)
    readonly property var tints: [Theme.accent, Theme.green, Theme.yellow, Theme.red, "#b48ead", "#88c0d0", "#d08770"]
    readonly property color tint: {
        let h = 0
        for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0
        return tints[h % tints.length]
    }
    width: size; height: size; radius: size / 2
    color: Qt.rgba(tint.r, tint.g, tint.b, 0.18)

    Text {
        visible: !face.ready
        anchors.centerIn: parent
        text: Store.initials(avatar.name)
        color: Qt.lighter(avatar.tint, 1.15)
        font.family: Theme.font
        font.pixelSize: Math.round(avatar.size * 0.36)
        font.weight: Font.DemiBold
    }
    Picture { id: face; anchors.fill: parent; source: avatar.photo; visible: avatar.photo !== "" }
}
